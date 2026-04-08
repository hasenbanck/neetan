//! Program Segment Prefix (PSP) creation, EXEC, terminate, process stack.

use crate::{
    CpuAccess, DiskIo, MemoryAccess, NeetanOs, OsState,
    filesystem::{fat::FatVolume, fat_dir},
    memory, set_iret_carry,
    tables::*,
};

fn hard_clear_text_vram(memory: &mut dyn MemoryAccess) {
    for row in 0u32..25 {
        for col in 0u32..80 {
            let offset = (row * 80 + col) * 2;
            memory.write_byte(0xA0000 + offset, 0x00);
            memory.write_byte(0xA0000 + offset + 1, 0x00);
            memory.write_byte(0xA2000 + offset, 0xE1);
            memory.write_byte(0xA2000 + offset + 1, 0x00);
        }
    }
}

/// Parameters parsed from the EXEC parameter block (ES:BX).
pub(crate) struct ExecParams {
    pub env_seg: u16,
    pub cmd_tail_addr: u32,
    pub fcb1_addr: u32,
    pub fcb2_addr: u32,
}

/// Saved context of a suspended parent process during nested EXEC.
pub(crate) struct ProcessContext {
    pub psp_segment: u16,
    pub return_ss: u16,
    pub return_sp: u16,
    pub saved_dta_seg: u16,
    pub saved_dta_off: u16,
}

/// Writes a 256-byte Program Segment Prefix at the given segment.
///
/// `psp_segment`: segment where the PSP is placed.
/// `parent_psp`: parent PSP segment (equals own segment for COMMAND.COM).
/// `env_segment`: segment of the environment block.
/// `mem_top`: segment of top of available memory (typically 0xA000).
pub(crate) fn write_psp(
    mem: &mut dyn MemoryAccess,
    psp_segment: u16,
    parent_psp: u16,
    env_segment: u16,
    mem_top: u16,
) {
    let base = (psp_segment as u32) << 4;

    // Zero the entire 256-byte PSP.
    let zeros = [0u8; 256];
    mem.write_block(base, &zeros);

    // +0x00: INT 20h instruction (CD 20)
    mem.write_byte(base + PSP_OFF_INT20, 0xCD);
    mem.write_byte(base + PSP_OFF_INT20 + 1, 0x20);

    // +0x02: Segment of memory top
    mem.write_word(base + PSP_OFF_MEM_TOP, mem_top);

    // +0x05: Far call to INT 21h dispatcher (CALL FAR PSP:0050h)
    // Opcode 9A = CALL FAR ptr16:16
    // Target is the INT 21h/RETF stub at PSP:0050h.
    mem.write_byte(base + PSP_OFF_FAR_CALL, 0x9A);
    mem.write_word(base + PSP_OFF_FAR_CALL + 1, 0x0050); // offset
    mem.write_word(base + PSP_OFF_FAR_CALL + 3, psp_segment); // segment

    // +0x0A: Saved INT 22h vector (read from IVT at 0x0088)
    let int22_off = mem.read_word(0x0088);
    let int22_seg = mem.read_word(0x008A);
    write_far_ptr(mem, base + PSP_OFF_INT22_VEC, int22_seg, int22_off);

    // +0x0E: Saved INT 23h vector (read from IVT at 0x008C)
    let int23_off = mem.read_word(0x008C);
    let int23_seg = mem.read_word(0x008E);
    write_far_ptr(mem, base + PSP_OFF_INT23_VEC, int23_seg, int23_off);

    // +0x12: Saved INT 24h vector (read from IVT at 0x0090)
    let int24_off = mem.read_word(0x0090);
    let int24_seg = mem.read_word(0x0092);
    write_far_ptr(mem, base + PSP_OFF_INT24_VEC, int24_seg, int24_off);

    // +0x16: Parent PSP segment
    mem.write_word(base + PSP_OFF_PARENT_PSP, parent_psp);

    // +0x18: Job File Table (20 bytes)
    // Handles 0-4 map to SFT indices 0-4, rest = 0xFF (closed).
    for i in 0..5u32 {
        mem.write_byte(base + PSP_OFF_JFT + i, i as u8);
    }
    for i in 5..20u32 {
        mem.write_byte(base + PSP_OFF_JFT + i, 0xFF);
    }

    // +0x2C: Environment segment
    mem.write_word(base + PSP_OFF_ENV_SEG, env_segment);

    // +0x32: Handle table size (WORD, default 20)
    mem.write_word(base + PSP_OFF_HANDLE_SIZE, 20);

    // +0x34: Far pointer to handle table (default: PSP:0018h)
    write_far_ptr(mem, base + PSP_OFF_HANDLE_PTR, psp_segment, 0x0018);

    // +0x50: INT 21h / RETF stub (CD 21 CB)
    mem.write_byte(base + PSP_OFF_INT21_STUB, 0xCD);
    mem.write_byte(base + PSP_OFF_INT21_STUB + 1, 0x21);
    mem.write_byte(base + PSP_OFF_INT21_STUB + 2, 0xCB);

    // +0x80: Command tail (length=0, terminated by CR)
    mem.write_byte(base + PSP_OFF_CMD_TAIL_LEN, 0x00);
    mem.write_byte(base + PSP_OFF_CMD_TAIL, 0x0D);
}

/// Writes the default COMMAND.COM environment block at the given segment.
///
/// Contents:
///   COMSPEC=Z:\COMMAND.COM\0
///   PATH=Z:\;A:\;B:\;C:\;\0
///   PROMPT=$P$G\0
///   \0                       (double-null terminator)
///   \x01\x00                 (WORD count = 1)
///   Z:\COMMAND.COM\0        (program pathname)
pub(crate) fn write_environment_block(mem: &mut dyn MemoryAccess, env_segment: u16) {
    let base = (env_segment as u32) << 4;

    // Zero the entire environment block first.
    let zeros = [0u8; ENV_BLOCK_PARAGRAPHS as usize * 16];
    mem.write_block(base, &zeros);

    let mut offset = 0u32;

    // COMSPEC=Z:\COMMAND.COM
    let comspec = b"COMSPEC=Z:\\COMMAND.COM";
    mem.write_block(base + offset, comspec);
    offset += comspec.len() as u32;
    mem.write_byte(base + offset, 0x00);
    offset += 1;

    // PATH=Z:\;A:\;B:\;C:\;
    let path = b"PATH=Z:\\;A:\\;B:\\;C:\\;";
    mem.write_block(base + offset, path);
    offset += path.len() as u32;
    mem.write_byte(base + offset, 0x00);
    offset += 1;

    // PROMPT=$P$G
    let prompt = b"PROMPT=$P$G";
    mem.write_block(base + offset, prompt);
    offset += prompt.len() as u32;
    mem.write_byte(base + offset, 0x00);
    offset += 1;

    // Double-null terminator (second NUL after last string's NUL)
    mem.write_byte(base + offset, 0x00);
    offset += 1;

    // WORD count = 1 (number of additional strings following)
    mem.write_word(base + offset, 0x0001);
    offset += 2;

    // Program pathname
    let pathname = b"Z:\\COMMAND.COM";
    mem.write_block(base + offset, pathname);
    offset += pathname.len() as u32;
    mem.write_byte(base + offset, 0x00);
}

/// Writes the COMMAND.COM code stub at PSP:0100h.
///
/// INT 28h (DOS Idle) is called on each iteration so that TSR programs
/// hooked to the idle interrupt get a chance to run.
///
/// ```text
/// loop:
///     MOV AH, FFh     ; B4 FF
///     INT 21h          ; CD 21
///     INT 28h          ; CD 28
///     JMP SHORT loop   ; EB F8
/// ```
pub(crate) fn write_command_com_stub(mem: &mut dyn MemoryAccess, psp_segment: u16) {
    let base = (psp_segment as u32) << 4;
    let entry = base + 0x0100;

    mem.write_byte(entry, 0xB4); // MOV AH, imm8
    mem.write_byte(entry + 1, 0xFF); // FFh
    mem.write_byte(entry + 2, 0xCD); // INT
    mem.write_byte(entry + 3, 0x21); // 21h
    mem.write_byte(entry + 4, 0xCD); // INT
    mem.write_byte(entry + 5, 0x28); // 28h
    mem.write_byte(entry + 6, 0xEB); // JMP SHORT
    mem.write_byte(entry + 7, 0xF8); // -8 (back to MOV AH)
}

/// Writes a child PSP with inherited handles, command tail, and FCBs.
pub(crate) fn write_child_psp(
    mem: &mut dyn MemoryAccess,
    child_psp: u16,
    parent_psp: u16,
    mem_top: u16,
    params: &ExecParams,
    sft_base: u32,
    sft2_base: u32,
) {
    let actual_env = if params.env_seg == 0 {
        let parent_base = (parent_psp as u32) << 4;
        mem.read_word(parent_base + PSP_OFF_ENV_SEG)
    } else {
        params.env_seg
    };

    write_psp(mem, child_psp, parent_psp, actual_env, mem_top);

    let child_base = (child_psp as u32) << 4;
    let parent_base = (parent_psp as u32) << 4;

    // Inherit parent's JFT and increment SFT ref counts.
    for i in 0..20u32 {
        let sft_index = mem.read_byte(parent_base + PSP_OFF_JFT + i);
        mem.write_byte(child_base + PSP_OFF_JFT + i, sft_index);
        if sft_index != 0xFF
            && let Some(sft_addr) = sft_entry_addr(sft_index, sft_base, sft2_base)
        {
            let ref_count = mem.read_word(sft_addr + SFT_ENT_REF_COUNT);
            mem.write_word(sft_addr + SFT_ENT_REF_COUNT, ref_count + 1);
        }
    }

    // Copy command tail: length byte at +0x80, text at +0x81.
    let tail_len = mem.read_byte(params.cmd_tail_addr);
    mem.write_byte(child_base + PSP_OFF_CMD_TAIL_LEN, tail_len);
    for i in 0..128u32.min(tail_len as u32 + 1) {
        let b = mem.read_byte(params.cmd_tail_addr + 1 + i);
        mem.write_byte(child_base + PSP_OFF_CMD_TAIL + i, b);
    }

    // Copy FCB1 (16 bytes) to PSP+0x5C.
    for i in 0..16u32 {
        let b = mem.read_byte(params.fcb1_addr + i);
        mem.write_byte(child_base + PSP_OFF_FCB1 + i, b);
    }

    // Copy FCB2 (16 bytes) to PSP+0x6C.
    for i in 0..16u32 {
        let b = mem.read_byte(params.fcb2_addr + i);
        mem.write_byte(child_base + PSP_OFF_FCB2 + i, b);
    }
}

/// Resolves an SFT entry address from its index, checking both SFT blocks.
fn sft_entry_addr(index: u8, sft_base: u32, sft2_base: u32) -> Option<u32> {
    let idx = index as u32;
    if idx < SFT_INITIAL_COUNT as u32 {
        Some(sft_base + SFT_HEADER_SIZE + idx * SFT_ENTRY_SIZE)
    } else if idx < SFT_TOTAL_COUNT as u32 {
        let local = idx - SFT_INITIAL_COUNT as u32;
        Some(sft2_base + SFT_HEADER_SIZE + local * SFT_ENTRY_SIZE)
    } else {
        None
    }
}

/// Reads entire file contents from a FAT volume by walking the cluster chain.
pub(crate) fn read_file_data(
    vol: &FatVolume,
    entry: &fat_dir::DirEntry,
    disk: &mut dyn DiskIo,
) -> Result<Vec<u8>, u16> {
    if entry.file_size == 0 {
        return Ok(Vec::new());
    }

    let mut data = Vec::with_capacity(entry.file_size as usize);
    let mut cluster = entry.start_cluster;

    loop {
        let chunk = vol.read_cluster(cluster, disk)?;
        data.extend_from_slice(&chunk);
        if data.len() >= entry.file_size as usize {
            break;
        }
        match vol.next_cluster(cluster) {
            Some(next) => cluster = next,
            None => break,
        }
    }

    data.truncate(entry.file_size as usize);
    Ok(data)
}

impl NeetanOs {
    /// INT 21h AH=4Bh: Execute program (EXEC).
    ///
    /// AL=00h: Load and execute.
    /// DS:DX = ASCIIZ program filename.
    /// ES:BX = parameter block (env_seg, cmd_tail, FCB1, FCB2).
    pub(crate) fn int21h_4bh_exec(
        &mut self,
        cpu: &mut dyn CpuAccess,
        mem: &mut dyn MemoryAccess,
        disk: &mut dyn DiskIo,
    ) {
        let al = (cpu.ax() & 0xFF) as u8;
        if al != 0x00 {
            cpu.set_ax(0x0001);
            set_iret_carry(cpu, mem, true);
            return;
        }

        let result = self.exec_load_and_execute(cpu, mem, disk);
        match result {
            Ok(()) => {}
            Err(error_code) => {
                cpu.set_ax(error_code);
                set_iret_carry(cpu, mem, true);
            }
        }
    }

    pub(crate) fn exec_load_and_execute(
        &mut self,
        cpu: &mut dyn CpuAccess,
        mem: &mut dyn MemoryAccess,
        disk: &mut dyn DiskIo,
    ) -> Result<(), u16> {
        // Parse parameters.
        let filename_addr = ((cpu.ds() as u32) << 4) + cpu.dx() as u32;
        let path = OsState::read_asciiz(mem, filename_addr, 128);

        let pb_addr = ((cpu.es() as u32) << 4) + cpu.bx() as u32;
        let env_seg = mem.read_word(pb_addr);
        let cmd_tail_off = mem.read_word(pb_addr + 2);
        let cmd_tail_seg = mem.read_word(pb_addr + 4);
        let fcb1_off = mem.read_word(pb_addr + 6);
        let fcb1_seg = mem.read_word(pb_addr + 8);
        let fcb2_off = mem.read_word(pb_addr + 10);
        let fcb2_seg = mem.read_word(pb_addr + 12);

        let params = ExecParams {
            env_seg,
            cmd_tail_addr: ((cmd_tail_seg as u32) << 4) + cmd_tail_off as u32,
            fcb1_addr: ((fcb1_seg as u32) << 4) + fcb1_off as u32,
            fcb2_addr: ((fcb2_seg as u32) << 4) + fcb2_off as u32,
        };

        // Resolve file path.
        let (drive_index, dir_cluster, fcb_name) =
            self.state.resolve_file_path(&path, mem, disk)?;

        // Z: drive contains only COMMAND.COM for COMSPEC compatibility.
        // All shell commands are built-in and resolved from the command registry,
        // never through EXEC. Return file-not-found for Z: drive EXEC attempts.
        if drive_index == 25 {
            return Err(0x0002);
        }

        // Find the file on disk.
        self.state.ensure_volume_mounted(drive_index, mem, disk)?;
        let vol = self.state.fat_volumes[drive_index as usize]
            .as_ref()
            .ok_or(0x0003u16)?;
        let dir_entry = fat_dir::find_entry(vol, dir_cluster, &fcb_name, disk)?.ok_or(0x0002u16)?;

        // Read file data.
        let file_data = read_file_data(vol, &dir_entry, disk)?;
        if file_data.is_empty() {
            return Err(0x000B);
        }

        // Detect .COM vs .EXE.
        let is_exe = file_data.len() >= 2
            && ((file_data[0] == 0x4D && file_data[1] == 0x5A)
                || (file_data[0] == 0x5A && file_data[1] == 0x4D));

        if is_exe {
            self.exec_exe(cpu, mem, &file_data, &params)
        } else {
            self.exec_com(cpu, mem, &file_data, &params)
        }
    }

    fn exec_com(
        &mut self,
        cpu: &mut dyn CpuAccess,
        mem: &mut dyn MemoryAccess,
        file_data: &[u8],
        params: &ExecParams,
    ) -> Result<(), u16> {
        let first_mcb = mem.read_word(self.state.sysvars_base - 2);

        // Allocate largest available block for the .COM program.
        let largest =
            match memory::allocate(mem, first_mcb, 0xFFFF, 0, self.state.allocation_strategy) {
                Ok(_) => unreachable!(),
                Err((_err, largest)) => largest,
            };

        if largest == 0 {
            return Err(0x0008);
        }

        let child_psp =
            memory::allocate(mem, first_mcb, largest, 0, self.state.allocation_strategy)
                .map_err(|(e, _)| e as u16)?;

        // Set MCB owner to the child PSP segment and name.
        let mcb_segment = child_psp - 1;
        mem.write_word(((mcb_segment as u32) << 4) + MCB_OFF_OWNER, child_psp);

        // Write child PSP with inherited handles, command tail, FCBs.
        write_child_psp(
            mem,
            child_psp,
            self.state.current_psp,
            MEMORY_TOP_SEGMENT,
            params,
            SFT_BASE,
            self.state.sft2_base,
        );

        // Load .COM data at PSP:0100h.
        let load_addr = ((child_psp as u32) << 4) + 0x0100;
        mem.write_block(load_addr, file_data);

        // Write safety return word (0x0000) at PSP:FFFEh.
        let stack_top = ((child_psp as u32) << 4) + 0xFFFE;
        mem.write_word(stack_top, 0x0000);

        // Perform context switch and build child IRET frame.
        self.exec_context_switch(cpu, mem, child_psp, child_psp, 0xFFF8);

        // Build IRET frame on child stack.
        let iret_base = ((child_psp as u32) << 4) + 0xFFF8;
        mem.write_word(iret_base, 0x0100); // IP
        mem.write_word(iret_base + 2, child_psp); // CS
        mem.write_word(iret_base + 4, 0x0202); // FLAGS (IF set)

        // Set segment registers for child.
        cpu.set_ds(child_psp);
        cpu.set_es(child_psp);

        Ok(())
    }

    fn exec_exe(
        &mut self,
        cpu: &mut dyn CpuAccess,
        mem: &mut dyn MemoryAccess,
        file_data: &[u8],
        params: &ExecParams,
    ) -> Result<(), u16> {
        if file_data.len() < 28 {
            return Err(0x000B);
        }

        // Parse MZ header.
        let bytes_last_page = u16::from_le_bytes([file_data[2], file_data[3]]);
        let total_pages = u16::from_le_bytes([file_data[4], file_data[5]]);
        let reloc_count = u16::from_le_bytes([file_data[6], file_data[7]]) as usize;
        let header_paragraphs = u16::from_le_bytes([file_data[8], file_data[9]]);
        let min_alloc = u16::from_le_bytes([file_data[10], file_data[11]]);
        let max_alloc = u16::from_le_bytes([file_data[12], file_data[13]]);
        let init_ss = u16::from_le_bytes([file_data[14], file_data[15]]);
        let init_sp = u16::from_le_bytes([file_data[16], file_data[17]]);
        // checksum at [18..20] is ignored
        let init_ip = u16::from_le_bytes([file_data[20], file_data[21]]);
        let init_cs = u16::from_le_bytes([file_data[22], file_data[23]]);
        let reloc_table_offset = u16::from_le_bytes([file_data[24], file_data[25]]) as usize;

        let header_size = (header_paragraphs as u32) * 16;
        let mut load_size = (total_pages as u32) * 512;
        if bytes_last_page != 0 {
            load_size -= 512 - bytes_last_page as u32;
        }
        if load_size < header_size {
            return Err(0x000B);
        }
        load_size -= header_size;

        let image_paragraphs = load_size.div_ceil(16) as u16;
        // Total = PSP (0x10 paragraphs) + image + extra
        let psp_paragraphs: u16 = 0x10;

        let first_mcb = mem.read_word(self.state.sysvars_base - 2);

        // Try with max_alloc first, fall back to min_alloc.
        let total_needed = psp_paragraphs
            .saturating_add(image_paragraphs)
            .saturating_add(max_alloc);
        let child_psp = match memory::allocate(
            mem,
            first_mcb,
            total_needed,
            0,
            self.state.allocation_strategy,
        ) {
            Ok(seg) => seg,
            Err(_) => {
                let min_needed = psp_paragraphs
                    .saturating_add(image_paragraphs)
                    .saturating_add(min_alloc);
                memory::allocate(
                    mem,
                    first_mcb,
                    min_needed,
                    0,
                    self.state.allocation_strategy,
                )
                .map_err(|(e, _)| e as u16)?
            }
        };

        // Set MCB owner to child PSP.
        let mcb_segment = child_psp - 1;
        mem.write_word(((mcb_segment as u32) << 4) + MCB_OFF_OWNER, child_psp);

        // Write child PSP.
        write_child_psp(
            mem,
            child_psp,
            self.state.current_psp,
            MEMORY_TOP_SEGMENT,
            params,
            SFT_BASE,
            self.state.sft2_base,
        );

        // Load EXE image after PSP (at child_psp + 0x10).
        let load_segment = child_psp + psp_paragraphs;
        let load_base = (load_segment as u32) << 4;
        let image_start = header_size as usize;
        let image_end = (image_start + load_size as usize).min(file_data.len());
        mem.write_block(load_base, &file_data[image_start..image_end]);

        // Apply segment relocations.
        for i in 0..reloc_count {
            let reloc_off = reloc_table_offset + i * 4;
            if reloc_off + 4 > file_data.len() {
                break;
            }
            let fixup_off = u16::from_le_bytes([file_data[reloc_off], file_data[reloc_off + 1]]);
            let fixup_seg =
                u16::from_le_bytes([file_data[reloc_off + 2], file_data[reloc_off + 3]]);
            let fixup_addr = (((load_segment + fixup_seg) as u32) << 4) + fixup_off as u32;
            let old_val = mem.read_word(fixup_addr);
            mem.write_word(fixup_addr, old_val.wrapping_add(load_segment));
        }

        // Compute entry point.
        let exe_cs = load_segment.wrapping_add(init_cs);
        let exe_ss = load_segment.wrapping_add(init_ss);

        // Build IRET frame on the EXE's stack.
        let iret_sp = init_sp.wrapping_sub(6);
        let iret_base = ((exe_ss as u32) << 4) + iret_sp as u32;
        mem.write_word(iret_base, init_ip); // IP
        mem.write_word(iret_base + 2, exe_cs); // CS
        mem.write_word(iret_base + 4, 0x0202); // FLAGS (IF set)

        // Context switch.
        self.exec_context_switch(cpu, mem, child_psp, exe_ss, iret_sp);

        // Set segment registers.
        cpu.set_ds(child_psp);
        cpu.set_es(child_psp);

        Ok(())
    }

    /// Saves parent context, updates IVT INT 22h, switches to child.
    fn exec_context_switch(
        &mut self,
        cpu: &mut dyn CpuAccess,
        mem: &mut dyn MemoryAccess,
        child_psp: u16,
        child_ss: u16,
        child_sp: u16,
    ) {
        // Read parent's return address from IRET frame for INT 22h.
        let ss_base = (cpu.ss() as u32) << 4;
        let sp = cpu.sp() as u32;
        let return_ip = mem.read_word(ss_base + sp);
        let return_cs = mem.read_word(ss_base + sp + 2);

        // Push parent context.
        self.state.process_stack.push(ProcessContext {
            psp_segment: self.state.current_psp,
            return_ss: cpu.ss(),
            return_sp: cpu.sp(),
            saved_dta_seg: self.state.dta_segment,
            saved_dta_off: self.state.dta_offset,
        });

        // Set IVT INT 22h to parent's return address.
        mem.write_word(0x0088, return_ip);
        mem.write_word(0x008A, return_cs);

        // Switch to child process.
        self.state.current_psp = child_psp;
        self.state.dta_segment = child_psp;
        self.state.dta_offset = 0x0080;

        // Point CPU at child's IRET frame.
        cpu.set_ss(child_ss);
        cpu.set_sp(child_sp);
    }

    /// Terminates the current process (normal termination).
    ///
    /// Closes JFT handles, frees MCBs, restores parent context and IVT vectors,
    /// then transfers control back to the parent via the saved IRET frame.
    pub(crate) fn terminate_process(
        &mut self,
        cpu: &mut dyn CpuAccess,
        mem: &mut dyn MemoryAccess,
        return_code: u8,
        termination_type: u8,
    ) {
        let child_psp_base = (self.state.current_psp as u32) << 4;

        // Close all JFT handles owned by the child.
        for handle in 0..20u16 {
            let sft_index = mem.read_byte(child_psp_base + PSP_OFF_JFT + handle as u32);
            if sft_index != 0xFF {
                self.state.free_handle(handle, mem);
            }
        }

        // Free all MCBs owned by the child.
        let first_mcb = mem.read_word(self.state.sysvars_base - 2);
        memory::free_process_blocks(mem, first_mcb, self.state.current_psp);

        // Restore IVT INT 22h/23h/24h from child PSP.
        let int22_off = mem.read_word(child_psp_base + PSP_OFF_INT22_VEC);
        let int22_seg = mem.read_word(child_psp_base + PSP_OFF_INT22_VEC + 2);
        mem.write_word(0x0088, int22_off);
        mem.write_word(0x008A, int22_seg);

        let int23_off = mem.read_word(child_psp_base + PSP_OFF_INT23_VEC);
        let int23_seg = mem.read_word(child_psp_base + PSP_OFF_INT23_VEC + 2);
        mem.write_word(0x008C, int23_off);
        mem.write_word(0x008E, int23_seg);

        let int24_off = mem.read_word(child_psp_base + PSP_OFF_INT24_VEC);
        let int24_seg = mem.read_word(child_psp_base + PSP_OFF_INT24_VEC + 2);
        mem.write_word(0x0090, int24_off);
        mem.write_word(0x0092, int24_seg);

        // Pop parent context.
        let parent = self
            .state
            .process_stack
            .pop()
            .expect("process stack underflow");
        self.state.current_psp = parent.psp_segment;
        self.state.dta_segment = parent.saved_dta_seg;
        self.state.dta_offset = parent.saved_dta_off;

        // Store return code.
        self.state.last_return_code = return_code;
        self.state.last_termination_type = termination_type;
        self.state.buffered_input = None;
        self.state.pending_key_bytes.clear();

        // Hard-clear text VRAM so the shell prompt starts on a clean screen.
        // Programs may leave arbitrary content in VRAM; the HLE shell re-renders
        // the prompt after this, so a full clear is safe.
        hard_clear_text_vram(mem);

        // Restore parent's IRET frame.
        cpu.set_ss(parent.return_ss);
        cpu.set_sp(parent.return_sp);
        set_iret_carry(cpu, mem, false);
    }

    /// Terminates with TSR: resize the PSP's MCB instead of freeing it.
    pub(crate) fn terminate_process_tsr(
        &mut self,
        cpu: &mut dyn CpuAccess,
        mem: &mut dyn MemoryAccess,
        return_code: u8,
        keep_paragraphs: u16,
    ) {
        let child_psp_base = (self.state.current_psp as u32) << 4;

        // Close all JFT handles owned by the child.
        for handle in 0..20u16 {
            let sft_index = mem.read_byte(child_psp_base + PSP_OFF_JFT + handle as u32);
            if sft_index != 0xFF {
                self.state.free_handle(handle, mem);
            }
        }

        // Resize PSP's MCB and free other blocks.
        let first_mcb = mem.read_word(self.state.sysvars_base - 2);
        memory::free_process_blocks_tsr(mem, first_mcb, self.state.current_psp, keep_paragraphs);

        // Restore IVT INT 22h/23h/24h from child PSP.
        let int22_off = mem.read_word(child_psp_base + PSP_OFF_INT22_VEC);
        let int22_seg = mem.read_word(child_psp_base + PSP_OFF_INT22_VEC + 2);
        mem.write_word(0x0088, int22_off);
        mem.write_word(0x008A, int22_seg);

        let int23_off = mem.read_word(child_psp_base + PSP_OFF_INT23_VEC);
        let int23_seg = mem.read_word(child_psp_base + PSP_OFF_INT23_VEC + 2);
        mem.write_word(0x008C, int23_off);
        mem.write_word(0x008E, int23_seg);

        let int24_off = mem.read_word(child_psp_base + PSP_OFF_INT24_VEC);
        let int24_seg = mem.read_word(child_psp_base + PSP_OFF_INT24_VEC + 2);
        mem.write_word(0x0090, int24_off);
        mem.write_word(0x0092, int24_seg);

        // Pop parent context.
        let parent = self
            .state
            .process_stack
            .pop()
            .expect("process stack underflow");
        self.state.current_psp = parent.psp_segment;
        self.state.dta_segment = parent.saved_dta_seg;
        self.state.dta_offset = parent.saved_dta_off;

        // Store return code.
        self.state.last_return_code = return_code;
        self.state.last_termination_type = 3;

        // Restore parent's IRET frame.
        cpu.set_ss(parent.return_ss);
        cpu.set_sp(parent.return_sp);
        set_iret_carry(cpu, mem, false);
    }
}

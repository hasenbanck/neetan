//! PC-9821Ra40 KVM-backed machine.
//!
//! Lives alongside the interpreter-backed [`Machine`](crate::Machine) wrapper
//! but does not reuse it, because the KVM execution model is fundamentally
//! different: instead of calling `cpu.run_for(&mut bus)` in a loop, the host
//! enters the guest via `KVM_RUN` and services vmexits against the bus.
//!
//! Only available on Linux x86_64 with the workspace `kvm` feature enabled.

mod cpu_shim;

use std::{
    path::Path,
    time::{Duration, Instant},
};

use common::{
    Bus as BusTrait, Cpu as CpuTrait, DisplaySnapshotUpload, Machine as MachineTrait, MachineModel,
    PegcSnapshotUpload, SegmentRegister,
};
use kvm::{
    BudgetTimer, Error as KvmError, KvmSystem, KvmVcpu, KvmVm, LeakedSlice, MemorySlotHandle,
    VmExit,
};

use crate::{
    NoTracing, Pc9801Bus, Tracing, machine::insert_cdrom_impl, ra40::cpu_shim::KvmCpuShim,
};

/// PC-9821Ra40 machine (Pentium II, PCI, 32 MiB RAM, Linux KVM backend).
///
/// Holds the KVM resources alongside the shared [`Pc9801Bus`]. All
/// [`common::Machine`] methods except [`run_for`](Self::run_for) delegate to
/// the bus, so the host application doesn't need to know which backend is
/// driving execution.
pub struct Pc9821Ra40<T: Tracing = NoTracing> {
    /// Opens `/dev/kvm`. Held for its lifetime.
    _kvm: KvmSystem,
    /// Owns the per-VM slot table. Must outlive the vCPU.
    _vm: KvmVm,
    /// The single vCPU running the guest.
    vcpu: KvmVcpu,
    /// Backs KVM memory slot 1 (system space at 0xA0000-0xFFFFF). Owns the
    /// host bytes that hold the BIOS ROM and VRAM regions visible to the
    /// guest. Not shared with [`Pc9801Memory`] — the POC does not round-trip
    /// VRAM back to the host-side rendering path.
    system_space: LeakedSlice,
    /// Handles for the three RAM slots, retained so slot IDs stay stable on
    /// ROM-bank switches (currently unused but wired for future work).
    _main_ram_slot: MemorySlotHandle,
    _system_space_slot: MemorySlotHandle,
    _extended_ram_slot: MemorySlotHandle,
    /// SIGALRM-delivered POSIX timer used to bound each `run_for` slice.
    ///
    /// `None` if the process could not create a timer (permissions or
    /// resource limits); in that case the loop relies on natural vmexits.
    budget_timer: Option<BudgetTimer>,
    /// The shared bus. RAM fields point into the KVM guest memory slots;
    /// every other device is identical to a PC-9821AP bus.
    pub bus: Pc9801Bus<T>,
    /// Total guest cycles consumed since construction. Advanced heuristically
    /// from the wall-clock duration of each `KVM_RUN` slice.
    accumulated_cycles: u64,
}

/// Disposition of a vmexit after dispatch.
enum HandleResult {
    /// Resume execution with another `KVM_RUN`.
    Continue,
    /// Return to the outer run_for loop (HLE pending, reset, etc.).
    Yield,
    /// Terminate the current `run_for` slice (shutdown, fatal error).
    Break,
}

/// PC-9821Ra40 low-memory layout.
const RA40_MAIN_RAM_START: u64 = 0x0000_0000;
const RA40_MAIN_RAM_SIZE: usize = 0xA_0000; // 640 KiB
const RA40_SYSTEM_SPACE_START: u64 = 0x000A_0000;
const RA40_SYSTEM_SPACE_SIZE: usize = 0x6_0000; // 384 KiB (VRAM, ROM, etc.)
const RA40_EXTENDED_RAM_START: u64 = 0x0010_0000;
/// BIOS ROM base within the system-space slice.
const BIOS_ROM_OFFSET_IN_SYSTEM_SPACE: usize = 0x0004_8000;

/// x86 initial reset vector (CS.base = FFFF0000, IP = FFF0) linear address.
const RESET_VECTOR_LINEAR: u32 = 0xFFFF_0000;
/// Guest physical address at which the BIOS stub ROM is visible.
///
/// On cold reset the vCPU starts at `CS:IP = F000:FFF0`. The CS descriptor
/// cache has base = `0xFFFF_0000`, so the first fetch lands at linear
/// `0xFFFF_FFF0`. On a real PC-98 this maps through chipset aliasing to the
/// top of the BIOS ROM at `0x000F_FFF0`. We replicate the alias by writing
/// the BIOS ROM image into the system-space slice at offset `0x4_8000`
/// (system-space begins at guest address `0xA_0000`, so the ROM sits at
/// `0xE_8000` - `0xF_FFFF`). A separate slot-1 alias at `0xFFFF_0000` would
/// be required for an authentic reset fetch; the POC punts on that by
/// explicitly re-anchoring CS to `0000:XXXX` in [`Pc9821Ra40::new`].
const _BIOS_ROM_GUEST_PHYS: u64 = 0x000E_8000;

/// Intel-recommended TSS address used by KVM's real-mode emulation helpers.
/// Must lie outside any registered memory slot; any value in the top GiB
/// works. Matches the value QEMU/Firecracker use.
const KVM_TSS_ADDRESS: usize = 0xFFFB_D000;

/// TSC-style approximation of cycles-per-second for the Ra40.
///
/// The real Pentium II in the Ra40 ticks at 400 MHz; we quote that
/// nominally so PIT scaling and scheduler math line up, even though the
/// host CPU runs the guest at native speed.
const RA40_CYCLES_PER_SECOND: u64 = 400_000_000;

impl<T: Tracing> Pc9821Ra40<T> {
    /// Builds a PC-9821Ra40 machine around an already-configured bus.
    ///
    /// The caller passes a [`Pc9801Bus`] built via [`Pc9801Bus::new`] for
    /// [`MachineModel::PC9821RA40`]. This constructor:
    ///
    /// 1. Opens `/dev/kvm` and creates a new VM + vCPU.
    /// 2. Allocates three KVM memory slots (main RAM, system space,
    ///    extended RAM), all backed by freshly-leaked host slices.
    /// 3. Copies the bus's current owned RAM contents into the KVM-leaked
    ///    slices and swaps the bus's `Pc9801Memory` backends to
    ///    `Borrowed(...)` via
    ///    [`Pc9801Bus::swap_ram_to_borrowed`].
    /// 4. Copies the currently-selected BIOS ROM image into the
    ///    system-space slot so the cold-reset fetch at linear `0xFFFF0`
    ///    lands on the ROM bytes.
    /// 5. Seeds a Pentium II CPUID view, minimal MSRs, and x86 cold-reset
    ///    register state.
    /// 6. Tries to create a SIGALRM-backed budget timer; on failure the
    ///    run loop falls back to natural-exit preemption.
    ///
    /// Returns an error if `/dev/kvm` cannot be opened, if the kernel API
    /// version is unsupported, or if any KVM ioctl fails.
    pub fn new(mut bus: Pc9801Bus<T>) -> Result<Self, KvmError> {
        let kvm = KvmSystem::open()?;
        let mut vm = kvm.create_vm()?;
        vm.set_tss_address(KVM_TSS_ADDRESS)?;

        // Allocate the three RAM regions. Each is its own leaked heap
        // slice; the KVM kernel mapping and the bus-side `Pc9801Memory`
        // accessors both point at the same bytes after the swap below.
        let mut main_ram = LeakedSlice::new_zeroed(RA40_MAIN_RAM_SIZE);
        let mut system_space = LeakedSlice::new_zeroed(RA40_SYSTEM_SPACE_SIZE);
        let extended_ram_size = MachineModel::PC9821RA40.extended_ram_default_size();
        let mut extended_ram = LeakedSlice::new_zeroed(extended_ram_size);

        // Register each region as a KVM memory slot at the right GPA.
        let main_ram_slot = vm.register_ram_slot_leaked(RA40_MAIN_RAM_START, &mut main_ram)?;
        let system_space_slot =
            vm.register_ram_slot_leaked(RA40_SYSTEM_SPACE_START, &mut system_space)?;
        let extended_ram_slot =
            vm.register_ram_slot_leaked(RA40_EXTENDED_RAM_START, &mut extended_ram)?;

        // Install the currently-selected BIOS ROM into the system-space
        // slice at offset 0x48000, placing it at guest physical
        // `0xE8000-0xFFFFF`.
        {
            let rom = bus.current_rom_image();
            let dest = &mut system_space.as_mut_slice()
                [BIOS_ROM_OFFSET_IN_SYSTEM_SPACE..BIOS_ROM_OFFSET_IN_SYSTEM_SPACE + rom.len()];
            dest.copy_from_slice(rom);
        }

        // Swap the bus's owned RAM backings for borrowed ones. Existing
        // RAM contents (set up by the bus's own `initialize_post_boot_state`,
        // `insert_hdd` etc.) are preserved.
        bus.swap_ram_to_borrowed(main_ram, extended_ram);

        // Spawn the vCPU. All HLE handlers see a Pentium II identity.
        let mut vcpu = vm.create_vcpu(0)?;
        seed_pentium2_cpuid(&kvm, &vcpu)?;
        seed_initial_msrs(&vcpu)?;
        seed_real_mode_reset_state(&mut vcpu)?;

        // Try to arm a SIGALRM timer. If `timer_create` is unavailable the
        // run-loop falls back to natural-exit preemption (slower but still
        // correct for short budgets).
        let budget_timer = BudgetTimer::new().ok();

        Ok(Self {
            _kvm: kvm,
            _vm: vm,
            vcpu,
            system_space,
            _main_ram_slot: main_ram_slot,
            _system_space_slot: system_space_slot,
            _extended_ram_slot: extended_ram_slot,
            budget_timer,
            bus,
            accumulated_cycles: 0,
        })
    }

    /// Copies the BIOS ROM bytes currently visible from the bus into the
    /// KVM-mapped system-space slot at offset `BIOS_ROM_OFFSET_IN_SYSTEM_SPACE`.
    ///
    /// Called on cold-reset and after `0x043D` port writes so the guest sees
    /// the ROM bank selected by the PC-98 bus.
    fn sync_rom_bank_to_system_space(&mut self) {
        let rom = self.bus.current_rom_image();
        let offset = BIOS_ROM_OFFSET_IN_SYSTEM_SPACE;
        self.system_space.as_mut_slice()[offset..offset + rom.len()].copy_from_slice(rom);
    }

    /// Returns `(cs_selector, cs_base, ip)` for the vCPU's current fetch
    /// address. Intended for diagnostics and integration tests.
    pub fn cs_ip_snapshot(&self) -> (u16, u32, u16) {
        let sregs = match self.vcpu.get_sregs() {
            Ok(s) => s,
            Err(_) => return (0, 0, 0),
        };
        let regs = match self.vcpu.get_regs() {
            Ok(r) => r,
            Err(_) => return (0, 0, 0),
        };
        (sregs.cs.selector, sregs.cs.base as u32, regs.rip as u16)
    }

    /// Reads `len` bytes of guest memory at `guest_phys_addr`, served from
    /// the KVM-mapped system-space slice.
    ///
    /// Returns `None` if the region is not covered. Useful for inspecting
    /// text VRAM (guest addresses `0xA0000-0xA3FFF`) without going through
    /// the bus-side (out-of-sync) `text_vram` buffer.
    pub fn peek_guest_memory(&self, guest_phys_addr: u64, len: usize) -> Option<Vec<u8>> {
        let system_end = RA40_SYSTEM_SPACE_START + RA40_SYSTEM_SPACE_SIZE as u64;
        if guest_phys_addr >= RA40_SYSTEM_SPACE_START && guest_phys_addr + len as u64 <= system_end
        {
            let offset = (guest_phys_addr - RA40_SYSTEM_SPACE_START) as usize;
            return Some(self.system_space.as_slice()[offset..offset + len].to_vec());
        }
        None
    }

    /// Runs the guest for up to `budget` CPU cycles.
    ///
    /// The loop interleaves:
    ///
    /// 1. Reset handling — when the guest wrote port 0xF0, drain the pending
    ///    reset context and re-seed vCPU state before the next `KVM_RUN`.
    /// 2. HLE dispatch — when a BIOS/SASI/IDE trap is pending, fetch the
    ///    vCPU register file into a [`KvmCpuShim`], run the existing HLE
    ///    handler, and flush any writes back.
    /// 3. IRQ injection — deliver any pending PIC interrupt, gated by
    ///    `ready_for_interrupt_injection`.
    /// 4. `KVM_RUN` — enter the guest until the next vmexit.
    /// 5. Exit dispatch — PIO traps route to `bus.io_{read,write}_*`; HLT,
    ///    interrupt-window, shutdown, and signal-interrupted exits each
    ///    steer the loop appropriately.
    ///
    /// Returns the number of emulated cycles consumed. Under KVM those are
    /// derived from wall-clock time scaled against the Pentium II nominal
    /// frequency.
    fn run_for_impl(&mut self, budget: u64) -> u64 {
        let start_cycle = self.bus.current_cycle();
        let deadline = start_cycle.saturating_add(budget);
        let start_instant = Instant::now();

        let budget_nanos = cycles_to_nanos(budget);
        if let Some(timer) = &self.budget_timer {
            let _ = timer.arm(Duration::from_nanos(budget_nanos));
        }

        loop {
            self.bus
                .set_cpu_protected_mode_enabled(read_vcpu_cr0_pe(&self.vcpu));

            let current = self.bus.current_cycle();
            if current >= deadline {
                break;
            }

            // 1. Reset handling.
            if let Some(warm_ctx) = self.bus.take_reset_pending() {
                if self.bus.shutdown_requested() {
                    break;
                }
                match warm_ctx {
                    Some((ss, sp, cs, ip)) => {
                        if self.apply_warm_reset(ss, sp, cs, ip).is_err() {
                            break;
                        }
                    }
                    None => {
                        self.bus.select_rom_bank_itf();
                        self.sync_rom_bank_to_system_space();
                        if self.apply_cold_reset().is_err() {
                            break;
                        }
                    }
                }
                continue;
            }

            // 2. Pending HLE dispatch.
            if self.bus.bios_hle_pending()
                || self.bus.sasi_hle_pending()
                || self.bus.ide_hle_pending()
            {
                if self.dispatch_pending_hle().is_err() {
                    break;
                }
                continue;
            }

            // 3. Interrupt injection.
            if self.bus.has_nmi() {
                self.bus.acknowledge_nmi();
                let _ = self.vcpu.inject_nmi();
            }
            if self.bus.has_irq() {
                if self.vcpu.ready_for_interrupt_injection() {
                    let vector = self.bus.acknowledge_irq();
                    let _ = self.vcpu.inject_irq(vector);
                } else {
                    self.vcpu.request_interrupt_window();
                }
            }

            // 4. Enter the guest.
            //
            // Dispatch is inlined here so that the `VmExit<'a>` borrow of
            // `self.vcpu`'s `kvm_run` mmap does not collide with the
            // `&mut self.bus` / `&mut self.system_space` reborrows that
            // handle the exit payload.
            let bus = &mut self.bus;
            let system_space = &mut self.system_space;
            let result = match self.vcpu.run() {
                Ok(exit) => handle_exit(bus, system_space, exit),
                Err(_) => HandleResult::Break,
            };

            // 5. Advance the virtual cycle counter from wall-clock time.
            let elapsed_cycles = nanos_to_cycles(start_instant.elapsed().as_nanos() as u64);
            let new_cycle = start_cycle
                .saturating_add(elapsed_cycles)
                .min(deadline.saturating_sub(1));
            self.bus.set_current_cycle(new_cycle);

            match result {
                HandleResult::Continue => {}
                HandleResult::Yield => continue,
                HandleResult::Break => break,
            }
        }

        if let Some(timer) = &self.budget_timer {
            let _ = timer.disarm();
        }

        self.bus.current_cycle().saturating_sub(start_cycle)
    }

    /// Fetches the vCPU register file, runs the highest-priority pending HLE
    /// handler, and commits any dirty register writes back.
    fn dispatch_pending_hle(&mut self) -> Result<(), KvmError> {
        let mut shim = KvmCpuShim::fetch(&self.vcpu)?;
        self.bus
            .set_hle_paging(common::Cpu::cr0(&shim), common::Cpu::cr3(&shim));
        if self.bus.bios_hle_pending() {
            self.bus.execute_bios_hle(&mut shim);
        } else if self.bus.sasi_hle_pending() {
            self.bus
                .execute_sasi_hle(shim.segment_base(SegmentRegister::SS), shim.sp());
        } else if self.bus.ide_hle_pending() {
            self.bus
                .execute_ide_hle(shim.segment_base(SegmentRegister::SS), shim.sp());
        }
        shim.commit(&self.vcpu)
    }

    /// Re-seeds the vCPU register file to the cold-reset state.
    fn apply_cold_reset(&mut self) -> Result<(), KvmError> {
        seed_real_mode_reset_state(&mut self.vcpu)
    }

    /// Implements the PC-98 warm-reset sequence in vCPU terms: `SS:SP`,
    /// `CS:IP` are loaded from the saved slots written by the ITF ROM.
    fn apply_warm_reset(&mut self, ss: u16, sp: u16, cs: u16, ip: u16) -> Result<(), KvmError> {
        let mut sregs = self.vcpu.get_sregs()?;
        sregs.cs.selector = cs;
        sregs.cs.base = u64::from(cs) << 4;
        sregs.ss.selector = ss;
        sregs.ss.base = u64::from(ss) << 4;
        self.vcpu.set_sregs(&sregs)?;

        let mut regs = self.vcpu.get_regs()?;
        regs.rip = u64::from(ip);
        regs.rsp = u64::from(sp);
        self.vcpu.set_regs(&regs)?;
        Ok(())
    }
}

/// Returns the low bit of the vCPU's CR0 register (the `PE` protected-mode
/// enable bit). Used to keep the bus's protected-mode tracker in sync.
fn read_vcpu_cr0_pe(vcpu: &KvmVcpu) -> bool {
    match vcpu.get_sregs() {
        Ok(sregs) => (sregs.cr0 & 1) != 0,
        Err(_) => false,
    }
}

/// Services a single vmexit against the bus and (for ROM bank writes) the
/// KVM-mapped system-space slice.
///
/// Structured as a free function so the caller can keep the `VmExit`'s
/// borrow of the vCPU's `kvm_run` mmap alive while reborrowing `self.bus`
/// and `self.system_space` (both disjoint fields of [`Pc9821Ra40`]).
fn handle_exit<T: Tracing>(
    bus: &mut Pc9801Bus<T>,
    system_space: &mut LeakedSlice,
    exit: VmExit<'_>,
) -> HandleResult {
    match exit {
        VmExit::IoIn { port, data } => {
            match data.len() {
                1 => {
                    data[0] = bus.io_read_byte(port);
                }
                2 => {
                    let value = bus.io_read_word(port);
                    data[0] = value as u8;
                    data[1] = (value >> 8) as u8;
                }
                4 => {
                    let low = u32::from(bus.io_read_word(port));
                    let high = u32::from(bus.io_read_word(port.wrapping_add(2)));
                    let value = low | (high << 16);
                    data[0] = value as u8;
                    data[1] = (value >> 8) as u8;
                    data[2] = (value >> 16) as u8;
                    data[3] = (value >> 24) as u8;
                }
                _ => {
                    for (offset, slot) in data.iter_mut().enumerate() {
                        *slot = bus.io_read_byte(port.wrapping_add(offset as u16));
                    }
                }
            }
            HandleResult::Continue
        }
        VmExit::IoOut { port, data } => {
            match data.len() {
                1 => bus.io_write_byte(port, data[0]),
                2 => {
                    let value = u16::from(data[0]) | (u16::from(data[1]) << 8);
                    bus.io_write_word(port, value);
                }
                4 => {
                    let low = u16::from(data[0]) | (u16::from(data[1]) << 8);
                    let high = u16::from(data[2]) | (u16::from(data[3]) << 8);
                    bus.io_write_word(port, low);
                    bus.io_write_word(port.wrapping_add(2), high);
                }
                _ => {
                    for (offset, byte) in data.iter().enumerate() {
                        bus.io_write_byte(port.wrapping_add(offset as u16), *byte);
                    }
                }
            }
            // ROM-bank latch: if the guest wrote `0x043D`, copy the newly
            // selected bank into the KVM memory slot so the next fetch
            // sees the right bytes.
            if port == 0x043D {
                let rom = bus.current_rom_image();
                let dest = &mut system_space.as_mut_slice()
                    [BIOS_ROM_OFFSET_IN_SYSTEM_SPACE..BIOS_ROM_OFFSET_IN_SYSTEM_SPACE + rom.len()];
                dest.copy_from_slice(rom);
            }
            HandleResult::Continue
        }
        VmExit::Hlt => {
            // Let the outer loop advance the cycle counter to the next
            // scheduled event; the PIC/PIT will eventually raise an IRQ and
            // wake the vCPU.
            HandleResult::Yield
        }
        VmExit::InterruptWindowOpen => HandleResult::Yield,
        VmExit::Interrupted | VmExit::MmioRead { .. } | VmExit::MmioWrite { .. } => {
            // MMIO traps shouldn't happen in the POC layout (all guest
            // addresses map to RAM slots), but handle them as yields.
            HandleResult::Yield
        }
        VmExit::Shutdown | VmExit::FailEntry { .. } | VmExit::InternalError => HandleResult::Break,
        VmExit::Other => HandleResult::Yield,
    }
}

/// Converts CPU cycles (at the Ra40 nominal 400 MHz clock) to nanoseconds.
fn cycles_to_nanos(cycles: u64) -> u64 {
    ((u128::from(cycles) * 1_000_000_000) / u128::from(RA40_CYCLES_PER_SECOND)) as u64
}

/// Converts elapsed nanoseconds back to CPU cycles.
fn nanos_to_cycles(nanos: u64) -> u64 {
    ((u128::from(nanos) * u128::from(RA40_CYCLES_PER_SECOND)) / 1_000_000_000) as u64
}

/// Rewrites the host-supported CPUID leaves into a Pentium II view and
/// installs them on the vCPU via `KVM_SET_CPUID2`.
fn seed_pentium2_cpuid(kvm: &KvmSystem, vcpu: &KvmVcpu) -> Result<(), KvmError> {
    let host_cpuid = kvm.supported_cpuid()?;
    let pentium2 = kvm::pentium2_cpuid(&host_cpuid)?;
    vcpu.set_cpuid2(&pentium2)?;
    Ok(())
}

/// Seeds the minimum MSR set a Pentium II guest needs before executing.
fn seed_initial_msrs(vcpu: &KvmVcpu) -> Result<(), KvmError> {
    // IA32_SYSENTER_CS / ESP / EIP default to 0 at power-on.
    const IA32_SYSENTER_CS: u32 = 0x174;
    const IA32_SYSENTER_ESP: u32 = 0x175;
    const IA32_SYSENTER_EIP: u32 = 0x176;
    vcpu.set_msrs(&[
        (IA32_SYSENTER_CS, 0),
        (IA32_SYSENTER_ESP, 0),
        (IA32_SYSENTER_EIP, 0),
    ])
}

/// Initializes the vCPU register state for an x86 cold reset.
///
/// Sets CS:IP = F000:FFF0 with CS.base = `0x000F_0000` (matching the
/// system-space slot mapping), CR0 = `0x6000_0010` (cache-disable + ET,
/// PE=0), and minimal segment descriptors. Omits the authentic
/// `0xFFFF_0000` CS.base alias so the POC boot fetch lands in the
/// system-space slot without requiring a second memory alias.
fn seed_real_mode_reset_state(vcpu: &mut KvmVcpu) -> Result<(), KvmError> {
    let mut sregs = vcpu.get_sregs()?;

    // Code segment: CS = F000 with base already pointing inside the
    // system-space slot (at guest 0xF_0000). Fetch starts at F000:FFF0 →
    // linear 0xF_FFF0, which lies inside our system-space slot.
    sregs.cs.selector = 0xF000;
    sregs.cs.base = 0x000F_0000;
    sregs.cs.limit = 0xFFFF;
    sregs.cs.type_ = 0x0B; // code, read/exec, accessed
    sregs.cs.present = 1;
    sregs.cs.s = 1;
    sregs.cs.dpl = 0;
    sregs.cs.db = 0;
    sregs.cs.l = 0;
    sregs.cs.g = 0;
    sregs.cs.unusable = 0;

    // Data segments: selector 0, base 0.
    for seg in [
        &mut sregs.ds,
        &mut sregs.es,
        &mut sregs.fs,
        &mut sregs.gs,
        &mut sregs.ss,
    ] {
        seg.selector = 0;
        seg.base = 0;
        seg.limit = 0xFFFF;
        seg.type_ = 0x03;
        seg.present = 1;
        seg.s = 1;
        seg.dpl = 0;
        seg.db = 0;
        seg.l = 0;
        seg.g = 0;
        seg.unusable = 0;
    }

    // Control registers: CR0 = 0x6000_0010 (ET + CD + NW + reserved bit 4).
    sregs.cr0 = 0x6000_0010;
    sregs.cr2 = 0;
    sregs.cr3 = 0;
    sregs.cr4 = 0;
    sregs.efer = 0;

    vcpu.set_sregs(&sregs)?;

    let mut regs = vcpu.get_regs()?;
    regs.rip = 0xFFF0;
    regs.rflags = 0x2;
    regs.rax = 0;
    regs.rbx = 0;
    regs.rcx = 0;
    regs.rdx = 0;
    regs.rsi = 0;
    regs.rdi = 0;
    regs.rsp = 0;
    regs.rbp = 0;
    vcpu.set_regs(&regs)?;

    // Suppress the unused-static warning in the module scope: the reset
    // vector constant is documentation for `seed_real_mode_reset_state`.
    let _ = RESET_VECTOR_LINEAR;

    Ok(())
}

impl<T: Tracing> MachineTrait for Pc9821Ra40<T> {
    fn cpu_clock_hz(&self) -> f64 {
        RA40_CYCLES_PER_SECOND as f64
    }

    fn run_for(&mut self, budget: u64) -> u64 {
        let cycles = self.run_for_impl(budget);
        self.accumulated_cycles = self.accumulated_cycles.saturating_add(cycles);
        cycles
    }

    fn shutdown_requested(&self) -> bool {
        self.bus.shutdown_requested()
    }

    fn snapshot_display(&self) -> &DisplaySnapshotUpload {
        self.bus.vsync_snapshot()
    }

    fn pegc_snapshot_display(&self) -> Option<&PegcSnapshotUpload> {
        self.bus.pegc_vsync_snapshot()
    }

    fn push_keyboard_scancode(&mut self, code: u8) {
        self.bus.push_keyboard_scancode(code);
    }

    fn push_mouse_delta(&mut self, dx: i16, dy: i16) {
        self.bus.push_mouse_delta(dx, dy);
    }

    fn set_mouse_buttons(&mut self, left: bool, right: bool, middle: bool) {
        self.bus.set_mouse_buttons(left, right, middle);
    }

    fn generate_audio_samples(&mut self, volume: f32, output: &mut [f32]) -> usize {
        self.bus.generate_audio_samples(volume, output)
    }

    fn take_font_rom_dirty(&mut self) -> bool {
        self.bus.take_font_rom_dirty()
    }

    fn font_rom_data(&self) -> &[u8] {
        self.bus.font_rom_data()
    }

    fn insert_floppy(&mut self, drive: usize, path: &Path) -> Result<String, String> {
        crate::machine::insert_floppy_impl(&mut self.bus, drive, path)
    }

    fn eject_floppy(&mut self, drive: usize) {
        self.bus.eject_floppy(drive);
    }

    fn insert_cdrom(&mut self, path: &Path) -> Result<String, String> {
        insert_cdrom_impl(&mut self.bus, path)
    }

    fn eject_cdrom(&mut self) {
        self.bus.eject_cdrom();
    }

    fn flush_floppies(&mut self) {
        self.bus.flush_all_floppies();
    }

    fn flush_hdds(&mut self) {
        self.bus.flush_all_hdds();
    }

    fn flush_printer(&mut self) {
        self.bus.flush_printer();
    }
}

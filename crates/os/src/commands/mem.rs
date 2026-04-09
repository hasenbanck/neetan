//! MEM command - displays memory usage information.

use crate::{
    DiskIo, IoAccess, OsState,
    commands::{Command, RunningCommand, StepResult, is_help_request},
    tables::*,
};

pub(crate) struct Mem;

impl Command for Mem {
    fn name(&self) -> &'static str {
        "MEM"
    }

    fn start(&self, args: &[u8]) -> Box<dyn RunningCommand> {
        Box::new(RunningMem {
            args: args.to_vec(),
        })
    }
}

struct RunningMem {
    args: Vec<u8>,
}

fn read_mcb_owner(mem: &dyn crate::MemoryAccess, segment: u16) -> u16 {
    mem.read_word((segment as u32) * 16 + MCB_OFF_OWNER)
}

fn read_mcb_size(mem: &dyn crate::MemoryAccess, segment: u16) -> u16 {
    mem.read_word((segment as u32) * 16 + MCB_OFF_SIZE)
}

fn read_mcb_type(mem: &dyn crate::MemoryAccess, segment: u16) -> u8 {
    mem.read_byte((segment as u32) * 16 + MCB_OFF_TYPE)
}

fn walk_mcb_chain(mem: &dyn crate::MemoryAccess, first_segment: u16) -> (u32, u32) {
    let mut used_paragraphs: u32 = 0;
    let mut free_paragraphs: u32 = 0;
    let mut current = first_segment;
    for _ in 0..4096 {
        let block_type = read_mcb_type(mem, current);
        if block_type != 0x4D && block_type != 0x5A {
            break;
        }
        let owner = read_mcb_owner(mem, current);
        let size = read_mcb_size(mem, current) as u32;
        if owner == MCB_OWNER_FREE {
            free_paragraphs += size;
        } else {
            used_paragraphs += size;
        }
        if block_type == 0x5A {
            break;
        }
        current = current + size as u16 + 1;
    }
    (used_paragraphs, free_paragraphs)
}

fn largest_free_block(mem: &dyn crate::MemoryAccess, first_segment: u16) -> u32 {
    let mut largest: u32 = 0;
    let mut current = first_segment;
    for _ in 0..4096 {
        let block_type = read_mcb_type(mem, current);
        if block_type != 0x4D && block_type != 0x5A {
            break;
        }
        let owner = read_mcb_owner(mem, current);
        let size = read_mcb_size(mem, current) as u32;
        if owner == MCB_OWNER_FREE && size > largest {
            largest = size;
        }
        if block_type == 0x5A {
            break;
        }
        current = current + size as u16 + 1;
    }
    largest
}

fn format_row(label: &str, total: u32, used: u32, free: u32) -> String {
    format!(
        "{:<17}  {:>7}K    {:>7}K    {:>7}K",
        label,
        format_number(total),
        format_number(used),
        format_number(free),
    )
}

fn format_ems_line(label: &str, kb: u32, bytes: u32) -> String {
    format!(
        "{:<20}  {:>7}K ({:>10} bytes)",
        label,
        format_number(kb),
        format_number(bytes),
    )
}

fn format_largest_line(label: &str, kb: u32, bytes: u32) -> String {
    format!(
        "{}  {:>7}K ({:>10} bytes)",
        label,
        format_number(kb),
        format_number(bytes),
    )
}

fn format_number(n: u32) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(ch);
    }
    result.chars().rev().collect()
}

impl RunningCommand for RunningMem {
    fn step(
        &mut self,
        state: &mut OsState,
        io: &mut IoAccess,
        _disk: &mut dyn DiskIo,
    ) -> StepResult {
        if is_help_request(&self.args) {
            io.println(b"Displays the amount of used and free memory in your system.");
            io.println(b"");
            io.println(b"MEM");
            return StepResult::Done(0);
        }

        let conv_total_kb: u32 = 640;
        let (conv_used_para, _) = walk_mcb_chain(io.memory, FIRST_MCB_SEGMENT);
        let conv_used_kb = (conv_used_para * 16).div_ceil(1024);
        let conv_free_kb = conv_total_kb.saturating_sub(conv_used_kb);

        let mm = state.memory_manager.as_ref();

        let umb_total_kb: u32;
        let umb_used_kb: u32;
        let umb_free_kb: u32;
        if let Some(manager) = mm {
            if manager.is_umb_enabled() {
                let (umb_used_para, umb_free_para) =
                    walk_mcb_chain(io.memory, UMB_FIRST_MCB_SEGMENT);
                umb_total_kb = ((umb_used_para + umb_free_para) * 16).div_ceil(1024);
                umb_used_kb = (umb_used_para * 16).div_ceil(1024);
                umb_free_kb = umb_total_kb.saturating_sub(umb_used_kb);
            } else {
                umb_total_kb = 0;
                umb_used_kb = 0;
                umb_free_kb = 0;
            }
        } else {
            umb_total_kb = 0;
            umb_used_kb = 0;
            umb_free_kb = 0;
        }

        let xms_total_kb = mm.map_or(0, |m| m.xms_total_kb());
        let xms_free_kb = mm.map_or(0, |m| m.xms_free_kb());
        let xms_used_kb = xms_total_kb.saturating_sub(xms_free_kb);

        let total_total = conv_total_kb + umb_total_kb + xms_total_kb;
        let total_used = conv_used_kb + umb_used_kb + xms_used_kb;
        let total_free = total_total.saturating_sub(total_used);

        let under_1m_total = conv_total_kb + umb_total_kb;
        let under_1m_used = conv_used_kb + umb_used_kb;
        let under_1m_free = under_1m_total.saturating_sub(under_1m_used);

        io.println(b"");
        io.println(b"Memory Type          Total  =     Used  +     Free");
        io.println(b"-----------------  --------    --------    --------");

        let line = format_row("Conventional", conv_total_kb, conv_used_kb, conv_free_kb);
        io.println(line.as_bytes());

        if umb_total_kb > 0 {
            let line = format_row("Upper", umb_total_kb, umb_used_kb, umb_free_kb);
            io.println(line.as_bytes());
        }

        if xms_total_kb > 0 {
            let line = format_row("Extended (XMS)", xms_total_kb, xms_used_kb, xms_free_kb);
            io.println(line.as_bytes());
        }

        io.println(b"-----------------  --------    --------    --------");
        let line = format_row("Total memory", total_total, total_used, total_free);
        io.println(line.as_bytes());

        io.println(b"");
        let line = format_row(
            "Total under 1 MB",
            under_1m_total,
            under_1m_used,
            under_1m_free,
        );
        io.println(line.as_bytes());

        if let Some(manager) = mm
            && manager.is_ems_enabled()
        {
            let ems_total = manager.ems_total_kb();
            let ems_free = manager.ems_free_kb();
            io.println(b"");
            let line = format_ems_line("Total Expanded (EMS)", ems_total, ems_total * 1024);
            io.println(line.as_bytes());
            let line = format_ems_line("Free Expanded (EMS)", ems_free, ems_free * 1024);
            io.println(line.as_bytes());
        }

        io.println(b"");
        let largest_conv = largest_free_block(io.memory, FIRST_MCB_SEGMENT);
        let largest_conv_bytes = largest_conv * 16;
        let largest_conv_kb = largest_conv_bytes / 1024;
        let line = format_largest_line(
            "Largest executable program size",
            largest_conv_kb,
            largest_conv_bytes,
        );
        io.println(line.as_bytes());

        if umb_total_kb > 0 {
            let largest_umb = largest_free_block(io.memory, UMB_FIRST_MCB_SEGMENT);
            let largest_umb_bytes = largest_umb * 16;
            let largest_umb_kb = largest_umb_bytes / 1024;
            let line = format_largest_line(
                "Largest free upper memory block",
                largest_umb_kb,
                largest_umb_bytes,
            );
            io.println(line.as_bytes());
        }

        if mm.is_some_and(|m| m.hma_is_allocated()) {
            io.println(b"MS-DOS is resident in the high memory area.");
        }

        io.println(b"");
        StepResult::Done(0)
    }
}

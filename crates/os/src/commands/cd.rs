//! CD / CHDIR command.

use crate::{
    DiskIo, IoAccess, OsState,
    commands::{Command, RunningCommand, StepResult},
    tables,
};

pub(crate) struct Cd;

impl Command for Cd {
    fn name(&self) -> &'static str {
        "CD"
    }

    fn aliases(&self) -> &'static [&'static str] {
        &["CHDIR"]
    }

    fn start(&self, args: &[u8]) -> Box<dyn RunningCommand> {
        Box::new(RunningCd {
            args: args.to_vec(),
        })
    }
}

struct RunningCd {
    args: Vec<u8>,
}

impl RunningCommand for RunningCd {
    fn step(
        &mut self,
        state: &mut OsState,
        io: &mut IoAccess,
        _disk: &mut dyn DiskIo,
    ) -> StepResult {
        let args = self.args.trim_ascii();

        if args.is_empty() {
            print_current_directory(state, io);
            return StepResult::Done(0);
        }

        match state.change_directory(io.memory, args) {
            Ok(()) => StepResult::Done(0),
            Err(_) => {
                let msg = b"Invalid directory\r\n";
                for &byte in msg {
                    io.output_byte(byte);
                }
                StepResult::Done(1)
            }
        }
    }
}

fn print_current_directory(state: &OsState, io: &mut IoAccess) {
    let cds_addr = tables::CDS_BASE + (state.current_drive as u32) * tables::CDS_ENTRY_SIZE;

    let mut path = Vec::new();
    for i in 0..67u32 {
        let byte = io.memory.read_byte(cds_addr + tables::CDS_OFF_PATH + i);
        if byte == 0 {
            break;
        }
        path.push(byte);
    }

    for &byte in &path {
        io.output_byte(byte);
    }
    io.output_byte(b'\r');
    io.output_byte(b'\n');
}

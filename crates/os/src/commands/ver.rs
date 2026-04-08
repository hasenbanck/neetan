//! VER command.

use crate::{
    DiskIo, IoAccess, OsState,
    commands::{Command, RunningCommand, StepResult},
};

pub(crate) struct Ver;

impl Command for Ver {
    fn name(&self) -> &'static str {
        "VER"
    }

    fn start(&self, _args: &[u8]) -> Box<dyn RunningCommand> {
        Box::new(RunningVer)
    }
}

struct RunningVer;

impl RunningCommand for RunningVer {
    fn step(
        &mut self,
        state: &mut OsState,
        io: &mut IoAccess,
        _disk: &mut dyn DiskIo,
    ) -> StepResult {
        let (major, minor) = state.version;
        let msg = format!("Neetan OS Version {}.{}\r\n\r\n", major, minor);
        for &byte in msg.as_bytes() {
            io.output_byte(byte);
        }
        StepResult::Done(0)
    }
}

//! ECHO command.

use crate::{
    DiskIo, IoAccess, OsState,
    commands::{Command, RunningCommand, StepResult},
};

pub(crate) struct Echo;

impl Command for Echo {
    fn name(&self) -> &'static str {
        "ECHO"
    }

    fn start(&self, args: &[u8]) -> Box<dyn RunningCommand> {
        Box::new(RunningEcho {
            text: args.to_vec(),
        })
    }
}

struct RunningEcho {
    text: Vec<u8>,
}

impl RunningCommand for RunningEcho {
    fn step(
        &mut self,
        _state: &mut OsState,
        io: &mut IoAccess,
        _disk: &mut dyn DiskIo,
    ) -> StepResult {
        if self.text.is_empty() {
            // ECHO. (bare dot) prints a blank line
            io.console.process_byte(io.memory, b'\r');
            io.console.process_byte(io.memory, b'\n');
        } else {
            for &byte in &self.text {
                io.console.process_byte(io.memory, byte);
            }
            io.console.process_byte(io.memory, b'\r');
            io.console.process_byte(io.memory, b'\n');
        }
        StepResult::Done(0)
    }
}

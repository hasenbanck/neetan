//! TIME command.

use crate::{
    DiskIo, IoAccess, OsState,
    commands::{Command, RunningCommand, StepResult, is_help_request},
};

pub(crate) struct Time;

impl Command for Time {
    fn name(&self) -> &'static str {
        "TIME"
    }

    fn start(&self, args: &[u8]) -> Box<dyn RunningCommand> {
        Box::new(RunningTime {
            args: args.to_vec(),
        })
    }
}

struct RunningTime {
    args: Vec<u8>,
}

impl RunningCommand for RunningTime {
    fn step(
        &mut self,
        _state: &mut OsState,
        io: &mut IoAccess,
        _disk: &mut dyn DiskIo,
    ) -> StepResult {
        if is_help_request(&self.args) {
            print_help(io);
            return StepResult::Done(0);
        }

        // Hardcoded DOS time: 0x6000 = 12:00:00
        let time: u16 = 0x6000;
        let hour = (time >> 11) & 0x1F;
        let minute = (time >> 5) & 0x3F;
        let second = (time & 0x1F) * 2;

        let msg = format!(
            "Current time is {:02}:{:02}:{:02}.00\r\n",
            hour, minute, second
        );
        for &byte in msg.as_bytes() {
            io.output_byte(byte);
        }
        StepResult::Done(0)
    }
}

fn print_help(io: &mut IoAccess) {
    io.println(b"Displays the time.");
    io.println(b"");
    io.println(b"TIME");
}

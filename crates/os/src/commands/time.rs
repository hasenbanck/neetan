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
        state: &mut OsState,
        io: &mut IoAccess,
        _disk: &mut dyn DiskIo,
    ) -> StepResult {
        if is_help_request(&self.args) {
            print_help(io);
            return StepResult::Done(0);
        }

        let (hour, minute, second) = state.current_time_parts();

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

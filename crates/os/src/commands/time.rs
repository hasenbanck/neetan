//! TIME command.

use crate::{
    DiskIo, IoAccess, OsState,
    commands::{Command, RunningCommand, StepResult},
};

pub(crate) struct Time;

impl Command for Time {
    fn name(&self) -> &'static str {
        "TIME"
    }

    fn start(&self, _args: &[u8]) -> Box<dyn RunningCommand> {
        Box::new(RunningTime)
    }
}

struct RunningTime;

impl RunningCommand for RunningTime {
    fn step(
        &mut self,
        _state: &mut OsState,
        io: &mut IoAccess,
        _disk: &mut dyn DiskIo,
    ) -> StepResult {
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

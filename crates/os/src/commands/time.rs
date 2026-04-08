//! TIME command.

use crate::commands::{Command, RunningCommand};

pub(crate) struct Time;

impl Command for Time {
    fn name(&self) -> &'static str {
        "TIME"
    }

    fn start(&self, _args: &[u8]) -> Box<dyn RunningCommand> {
        unimplemented!("TIME command")
    }
}

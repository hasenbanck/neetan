//! DIR command.

use crate::commands::{Command, RunningCommand};

pub(crate) struct Dir;

impl Command for Dir {
    fn name(&self) -> &'static str {
        "DIR"
    }

    fn start(&self, _args: &[u8]) -> Box<dyn RunningCommand> {
        unimplemented!("DIR command")
    }
}

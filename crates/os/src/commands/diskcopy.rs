//! DISKCOPY command.

use crate::commands::{Command, RunningCommand};

pub(crate) struct Diskcopy;

impl Command for Diskcopy {
    fn name(&self) -> &'static str {
        "DISKCOPY"
    }

    fn start(&self, _args: &[u8]) -> Box<dyn RunningCommand> {
        unimplemented!("DISKCOPY command")
    }
}

//! XCOPY command.

use crate::commands::{Command, RunningCommand};

pub(crate) struct Xcopy;

impl Command for Xcopy {
    fn name(&self) -> &'static str {
        "XCOPY"
    }

    fn start(&self, _args: &[u8]) -> Box<dyn RunningCommand> {
        unimplemented!("XCOPY command")
    }
}

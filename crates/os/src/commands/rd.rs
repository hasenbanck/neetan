//! RD / RMDIR command.

use crate::commands::{Command, RunningCommand};

pub(crate) struct Rd;

impl Command for Rd {
    fn name(&self) -> &'static str {
        "RD"
    }

    fn aliases(&self) -> &'static [&'static str] {
        &["RMDIR"]
    }

    fn start(&self, _args: &[u8]) -> Box<dyn RunningCommand> {
        unimplemented!("RD command")
    }
}

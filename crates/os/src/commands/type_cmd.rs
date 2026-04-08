//! TYPE command.

use crate::commands::{Command, RunningCommand};

pub(crate) struct TypeCmd;

impl Command for TypeCmd {
    fn name(&self) -> &'static str {
        "TYPE"
    }

    fn start(&self, _args: &[u8]) -> Box<dyn RunningCommand> {
        unimplemented!("TYPE command")
    }
}

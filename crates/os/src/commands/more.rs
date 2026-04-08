//! MORE command.

use crate::commands::{Command, RunningCommand};

pub(crate) struct More;

impl Command for More {
    fn name(&self) -> &'static str {
        "MORE"
    }

    fn start(&self, _args: &[u8]) -> Box<dyn RunningCommand> {
        unimplemented!("MORE command")
    }
}

//! FORMAT command.

use crate::commands::{Command, RunningCommand};

pub(crate) struct Format;

impl Command for Format {
    fn name(&self) -> &'static str {
        "FORMAT"
    }

    fn start(&self, _args: &[u8]) -> Box<dyn RunningCommand> {
        unimplemented!("FORMAT command")
    }
}

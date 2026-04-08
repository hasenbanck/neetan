//! COPY command.

use crate::commands::{Command, RunningCommand};

pub(crate) struct Copy;

impl Command for Copy {
    fn name(&self) -> &'static str {
        "COPY"
    }

    fn start(&self, _args: &[u8]) -> Box<dyn RunningCommand> {
        unimplemented!("COPY command")
    }
}

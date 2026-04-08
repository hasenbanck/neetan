//! DATE command.

use crate::commands::{Command, RunningCommand};

pub(crate) struct Date;

impl Command for Date {
    fn name(&self) -> &'static str {
        "DATE"
    }

    fn start(&self, _args: &[u8]) -> Box<dyn RunningCommand> {
        unimplemented!("DATE command")
    }
}

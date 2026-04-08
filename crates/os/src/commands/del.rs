//! DEL / ERASE command.

use crate::commands::{Command, RunningCommand};

pub(crate) struct Del;

impl Command for Del {
    fn name(&self) -> &'static str {
        "DEL"
    }

    fn aliases(&self) -> &'static [&'static str] {
        &["ERASE"]
    }

    fn start(&self, _args: &[u8]) -> Box<dyn RunningCommand> {
        unimplemented!("DEL command")
    }
}

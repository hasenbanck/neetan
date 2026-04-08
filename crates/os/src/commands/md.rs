//! MD / MKDIR command.

use crate::commands::{Command, RunningCommand};

pub(crate) struct Md;

impl Command for Md {
    fn name(&self) -> &'static str {
        "MD"
    }

    fn aliases(&self) -> &'static [&'static str] {
        &["MKDIR"]
    }

    fn start(&self, _args: &[u8]) -> Box<dyn RunningCommand> {
        unimplemented!("MD command")
    }
}

//! REM command.

use crate::{
    DiskIo, IoAccess, OsState,
    commands::{Command, RunningCommand, StepResult},
};

pub(crate) struct Rem;

impl Command for Rem {
    fn name(&self) -> &'static str {
        "REM"
    }

    fn start(&self, _args: &[u8]) -> Box<dyn RunningCommand> {
        Box::new(RunningRem)
    }
}

struct RunningRem;

impl RunningCommand for RunningRem {
    fn step(
        &mut self,
        _state: &mut OsState,
        _io: &mut IoAccess,
        _disk: &mut dyn DiskIo,
    ) -> StepResult {
        StepResult::Done(0)
    }
}

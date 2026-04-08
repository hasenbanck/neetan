//! CLS command.

use crate::{
    DiskIo, IoAccess, OsState,
    commands::{Command, RunningCommand, StepResult},
};

pub(crate) struct Cls;

impl Command for Cls {
    fn name(&self) -> &'static str {
        "CLS"
    }

    fn start(&self, _args: &[u8]) -> Box<dyn RunningCommand> {
        Box::new(RunningCls)
    }
}

struct RunningCls;

impl RunningCommand for RunningCls {
    fn step(
        &mut self,
        _state: &mut OsState,
        io: &mut IoAccess,
        _disk: &mut dyn DiskIo,
    ) -> StepResult {
        io.console.clear_screen(io.memory);
        StepResult::Done(0)
    }
}

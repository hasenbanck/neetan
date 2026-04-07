//! INT 20h: Program Terminate.

use crate::{CpuAccess, MemoryAccess, NeetanOs};

impl NeetanOs {
    /// INT 20h: Terminate current program.
    /// Sets return code to 0 and termination type to 0 (normal).
    /// TODO: Full process teardown (restoring parent PSP, freeing memory, INT 22h transfer)
    ///       is deferred to the process management phase.
    pub(crate) fn int20h(&mut self, _cpu: &mut dyn CpuAccess, _memory: &mut dyn MemoryAccess) {
        self.last_return_code = 0;
        self.last_termination_type = 0;
        unimplemented!("INT 20h: full process teardown not yet implemented");
    }
}

//! INT 29h: DOS Fast Console Output.
//!
//! Outputs the character in AL directly to the console, bypassing normal
//! DOS I/O buffering. Actual display output is deferred to the console
//! implementation phase (step 10.7).

use crate::{CpuAccess, MemoryAccess, NeetanOs};

impl NeetanOs {
    /// INT 29h: Fast console output stub. Character is in AL.
    pub(crate) fn int29h(&self, _cpu: &mut dyn CpuAccess, _memory: &mut dyn MemoryAccess) {}
}

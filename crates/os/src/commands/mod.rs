//! Command trait definitions and command registry.
//!
//! All shell commands implement the unified Command/RunningCommand trait system.
//! Commands are stateless factories (Command) that produce stateful running
//! instances (RunningCommand). The shell calls step() once per INT 21h AH=FFh
//! dispatch -- commands must return quickly and never block.

pub mod cd;
pub mod cls;
pub mod copy;
pub mod date;
pub mod del;
pub mod dir;
pub mod diskcopy;
pub mod echo;
pub mod format;
pub mod md;
pub mod more;
pub mod rd;
pub mod rem;
pub mod set;
pub mod time;
pub mod type_cmd;
pub mod ver;
pub mod xcopy;

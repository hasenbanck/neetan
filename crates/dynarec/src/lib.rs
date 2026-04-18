//! Dynamic recompiler for the 386/486 CPU core.
//!
//! The entry point is [`I386Jit`]: it implements [`common::Cpu`] and can
//! be substituted for [`cpu::I386`] in a [`machine::Machine`].

#![warn(missing_docs)]

mod backend_bytecode;
#[cfg(all(target_arch = "x86_64", unix))]
mod backend_x64;
mod block_map;
mod code_cache;
mod decoder;
mod ir;
mod jit;

pub use jit::{I386Jit, JitBackend, JitStats};

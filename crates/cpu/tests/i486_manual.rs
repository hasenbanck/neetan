//! Integration test corpus derived from the Intel 80486 Programmer's Reference Manual.
//!
//! Each per-instruction (or per-feature) submodule lives under `i486_manual/`.
//! Submodules are pulled in by `#[path = "..."]` so this dispatcher is the
//! single test-binary entry point: keeping all corpus tests in one binary
//! avoids paying the linker cost for one binary per submodule.

#[path = "i486_manual/setup.rs"]
mod setup;

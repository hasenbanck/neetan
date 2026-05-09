//! Integration test corpus derived from the Intel 80486 Programmer's Reference Manual.

#[path = "i486_manual/setup.rs"]
mod setup;

#[path = "i486_manual/system_control.rs"]
mod system_control;

#[path = "i486_manual/int_n.rs"]
mod int_n;

#[path = "i486_manual/int3_into_bound.rs"]
mod int3_into_bound;

#[path = "i486_manual/iret.rs"]
mod iret;

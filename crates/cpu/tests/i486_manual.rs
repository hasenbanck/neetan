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

#[path = "i486_manual/popf_pushf.rs"]
mod popf_pushf;

#[path = "i486_manual/cli_sti.rs"]
mod cli_sti;

#[path = "i486_manual/in_out.rs"]
mod in_out;

#[path = "i486_manual/lar_lsl.rs"]
mod lar_lsl;

#[path = "i486_manual/mov_sreg.rs"]
mod mov_sreg;

#[path = "i486_manual/lds_les_lfs_lgs_lss.rs"]
mod lds_les_lfs_lgs_lss;

#[path = "i486_manual/i486_specific.rs"]
mod i486_specific;

#[path = "i486_manual/call_jmp_far.rs"]
mod call_jmp_far;

#[path = "i486_manual/long_tail.rs"]
mod long_tail;

#[path = "i486_manual/real_mode_addressing.rs"]
mod real_mode_addressing;

#[path = "i486_manual/real_mode_instruction_semantics.rs"]
mod real_mode_instruction_semantics;

#[path = "i486_manual/v86_addressing_and_faults.rs"]
mod v86_addressing_and_faults;

#[path = "i486_manual/v86_mode_transitions.rs"]
mod v86_mode_transitions;

#[path = "i486_manual/mixed_16_32.rs"]
mod mixed_16_32;

//! Hardware device emulations for PC-98 peripherals.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

pub mod beeper;
pub mod bios;
pub mod cgrom;
pub mod disk;
pub mod display_control;
pub mod egc;
pub mod floppy;
pub mod grcg;
pub mod i8237_dma;
pub mod i8251_keyboard;
pub mod i8251_serial;
pub mod i8253_pit;
pub mod i8255_mouse_ppi;
pub mod i8255_system_ppi;
pub mod i8259a_pic;
pub mod palette;
pub mod printer;
pub mod sasi;
pub mod soundboard_26k;
pub mod soundboard_86;
pub mod upd4990a_rtc;
pub mod upd52611_crtc;
pub mod upd7220_gdc;
pub mod upd765a_fdc;

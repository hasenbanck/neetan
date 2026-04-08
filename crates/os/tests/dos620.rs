#[path = "dos620/harness.rs"]
mod harness;

#[path = "dos620/memory_layout.rs"]
mod memory_layout;

#[path = "dos620/sysvars.rs"]
mod sysvars;

#[path = "dos620/iosys_workarea.rs"]
mod iosys_workarea;

#[path = "dos620/mcb_chain.rs"]
mod mcb_chain;

#[path = "dos620/psp.rs"]
mod psp;

#[path = "dos620/environment.rs"]
mod environment;

#[path = "dos620/drives.rs"]
mod drives;

#[path = "dos620/config.rs"]
mod config;

#[path = "dos620/compatibility.rs"]
mod compatibility;

#[path = "dos620/syscalls_int21h.rs"]
mod syscalls_int21h;

#[path = "dos620/syscalls_int21h_file_io.rs"]
mod syscalls_int21h_file_io;

#[path = "dos620/syscalls_int21h_console.rs"]
mod syscalls_int21h_console;

#[path = "dos620/data_structures.rs"]
mod data_structures;

#[path = "dos620/syscalls_int2fh.rs"]
mod syscalls_int2fh;

#[path = "dos620/syscalls_intdch.rs"]
mod syscalls_intdch;

#[path = "dos620/hdd_file_io.rs"]
mod hdd_file_io;

#[path = "dos620/process_management.rs"]
mod process_management;

#[path = "dos620/shell.rs"]
mod shell;

#[path = "dos620/commands_dir.rs"]
mod commands_dir;

#[path = "dos620/commands_copy.rs"]
mod commands_copy;

#[path = "dos620/commands_file_ops.rs"]
mod commands_file_ops;

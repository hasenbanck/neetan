use std::ffi::CStr;

/// Returns the version of the linked SDL3 library as (major, minor, patch).
pub fn version() -> (i32, i32, i32) {
    let v = sdl3_sys::version::SDL_GetVersion() as i32;
    (v / 1_000_000, (v / 1_000) % 1_000, v % 1_000)
}

/// Returns the revision (git hash) of the linked SDL3 library.
pub fn revision() -> &'static str {
    // Safety: SDL_GetRevision returns a static C string.
    unsafe {
        CStr::from_ptr(sdl3_sys::version::SDL_GetRevision())
            .to_str()
            .unwrap_or("")
    }
}

/// Returns the name of the platform (e.g. "macOS", "Windows", "Linux").
pub fn platform() -> &'static str {
    // Safety: SDL_GetPlatform returns a static C string.
    unsafe {
        CStr::from_ptr(sdl3_sys::platform::SDL_GetPlatform())
            .to_str()
            .unwrap_or("Unknown")
    }
}

/// Returns the number of logical CPU cores.
pub fn num_logical_cpu_cores() -> i32 {
    // Safety: Thread-safe, no preconditions.
    (unsafe { sdl3_sys::cpuinfo::SDL_GetNumLogicalCPUCores() }) as i32
}

/// Returns the amount of system RAM in MiB.
pub fn system_ram() -> i32 {
    // Safety: Thread-safe, no preconditions.
    (unsafe { sdl3_sys::cpuinfo::SDL_GetSystemRAM() }) as i32
}

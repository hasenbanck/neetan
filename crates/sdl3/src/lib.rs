//! Safe wrapper around SDL3.
#![deny(missing_docs)]

/// Audio subsystem.
pub mod audio;
/// Event polling.
pub mod event;
/// System information queries (version, platform, CPU, RAM).
pub mod info;
/// Keyboard scancodes and modifiers.
pub mod keyboard;
/// SDL3 log output capture.
pub mod log;
/// Mouse button types.
pub mod mouse;
/// Date and time utilities.
pub mod time;
/// Video subsystem, window creation, and Vulkan surface management.
pub mod video;

use std::{ffi::CStr, marker::PhantomData};

/// An SDL3 error message.
#[derive(Debug)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

/// Returns the last SDL3 error as an [`Error`].
pub fn get_error() -> Error {
    // Safety: SDL_GetError always returns a valid C string pointer.
    let msg = unsafe {
        let ptr = sdl3_sys::error::SDL_GetError();
        if ptr.is_null() {
            String::new()
        } else {
            CStr::from_ptr(ptr).to_string_lossy().into_owned()
        }
    };
    Error(msg)
}

/// Initializes SDL3 and returns the top-level context.
pub fn init() -> Result<Sdl, Error> {
    // Safety: SDL_Init(0) performs base initialization without any subsystems.
    let ok = unsafe { sdl3_sys::init::SDL_Init(sdl3_sys::init::SDL_InitFlags(0)) };
    if !ok {
        return Err(get_error());
    }
    Ok(Sdl {
        _marker: PhantomData,
    })
}

/// Top-level SDL3 context. Calls `SDL_Quit` on drop.
pub struct Sdl {
    _marker: PhantomData<*mut ()>,
}

impl Sdl {
    /// Initializes the audio subsystem.
    pub fn audio(&self) -> Result<audio::AudioSubsystem, Error> {
        audio::AudioSubsystem::new()
    }

    /// Initializes the video subsystem.
    pub fn video(&self) -> Result<video::VideoSubsystem, Error> {
        video::VideoSubsystem::new()
    }

    /// Creates an event pump for polling input events.
    pub fn event_pump(&self) -> Result<event::EventPump, Error> {
        event::EventPump::new()
    }
}

impl Drop for Sdl {
    fn drop(&mut self) {
        // Safety: Matches the SDL_Init call in init().
        unsafe { sdl3_sys::init::SDL_Quit() }
    }
}

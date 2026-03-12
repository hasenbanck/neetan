/// A mouse button.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum MouseButton {
    /// Left mouse button.
    Left,
    /// Middle mouse button.
    Middle,
    /// Right mouse button.
    Right,
    /// Extra button 1.
    X1,
    /// Extra button 2.
    X2,
    /// An unrecognized button.
    Unknown,
}

impl MouseButton {
    /// Converts a raw SDL3 button code to a `MouseButton`.
    pub fn from_raw(button: u8) -> Self {
        match button as i32 {
            sdl3_sys::mouse::SDL_BUTTON_LEFT => Self::Left,
            sdl3_sys::mouse::SDL_BUTTON_MIDDLE => Self::Middle,
            sdl3_sys::mouse::SDL_BUTTON_RIGHT => Self::Right,
            sdl3_sys::mouse::SDL_BUTTON_X1 => Self::X1,
            sdl3_sys::mouse::SDL_BUTTON_X2 => Self::X2,
            _ => Self::Unknown,
        }
    }
}

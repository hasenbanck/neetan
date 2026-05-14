//! Clipboard text I/O.

use std::ffi::CString;

use crate::{Error, get_error};

/// Sets the system clipboard to the given UTF-8 text.
///
/// Returns an error if SDL3 reports a clipboard failure or if `text`
/// contains an interior NUL byte.
pub fn set_text(text: &str) -> Result<(), Error> {
    let c_string = CString::new(text)
        .map_err(|_| Error("clipboard text contained an interior NUL byte".into()))?;
    // Safety: SDL_SetClipboardText reads the C string pointer and copies the
    // contents internally. The pointer is valid for the duration of the call.
    let ok = unsafe { sdl3_sys::clipboard::SDL_SetClipboardText(c_string.as_ptr()) };
    if ok { Ok(()) } else { Err(get_error()) }
}

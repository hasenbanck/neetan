use std::{
    ffi::{CStr, CString},
    marker::PhantomData,
};

use sdl3_sys::properties as ffi;

use crate::Error;

/// A group of SDL3 properties.
pub struct Properties {
    id: ffi::SDL_PropertiesID,
    _marker: PhantomData<*mut ()>,
}

impl Properties {
    /// Creates an empty property group.
    pub fn new() -> Result<Self, Error> {
        // Safety: SDL_CreateProperties takes no arguments.
        let id = unsafe { ffi::SDL_CreateProperties() };
        if id.0 == 0 {
            return Err(crate::get_error());
        }
        Ok(Self {
            id,
            _marker: PhantomData,
        })
    }

    /// Returns the raw `SDL_PropertiesID` value.
    pub fn as_raw(&self) -> ffi::SDL_PropertiesID {
        self.id
    }

    /// Sets a boolean property by C-string name (suitable for the
    /// `SDL_PROP_*` constants exported by `sdl3-sys`).
    pub fn set_bool_cstr(&self, name: &CStr, value: bool) -> Result<(), Error> {
        // Safety: id is valid; name is a valid C string pointer.
        let ok = unsafe { ffi::SDL_SetBooleanProperty(self.id, name.as_ptr(), value) };
        if !ok {
            return Err(crate::get_error());
        }
        Ok(())
    }

    /// Sets a numeric (signed 64-bit) property by C-string name.
    pub fn set_number_cstr(&self, name: &CStr, value: i64) -> Result<(), Error> {
        // Safety: id is valid; name is a valid C string pointer.
        let ok = unsafe { ffi::SDL_SetNumberProperty(self.id, name.as_ptr(), value) };
        if !ok {
            return Err(crate::get_error());
        }
        Ok(())
    }

    /// Sets a string property. The value is copied internally by SDL.
    pub fn set_string(&self, name: &CStr, value: &str) -> Result<(), Error> {
        let value_c = CString::new(value).map_err(|_| Error("string contains NUL".to_string()))?;
        // Safety: id is valid; both name and value_c are valid C string pointers.
        let ok = unsafe { ffi::SDL_SetStringProperty(self.id, name.as_ptr(), value_c.as_ptr()) };
        if !ok {
            return Err(crate::get_error());
        }
        Ok(())
    }
}

impl Drop for Properties {
    fn drop(&mut self) {
        // Safety: id was produced by SDL_CreateProperties and we own it.
        unsafe { ffi::SDL_DestroyProperties(self.id) }
    }
}

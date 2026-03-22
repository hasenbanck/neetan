use std::{
    ffi::{CStr, CString},
    marker::PhantomData,
};

pub use sdl3_sys::vulkan::VkInstance;
use sdl3_sys::{init, mouse as mouse_ffi, video as ffi, vulkan};

use crate::Error;

/// Manages the SDL3 video subsystem. Calls `SDL_QuitSubSystem(VIDEO)` on drop.
pub struct VideoSubsystem {
    _marker: PhantomData<*mut ()>,
}

impl VideoSubsystem {
    pub(crate) fn new() -> Result<Self, Error> {
        // Safety: Called from the main thread after SDL_Init.
        let ok = unsafe { init::SDL_InitSubSystem(init::SDL_INIT_VIDEO) };
        if !ok {
            return Err(crate::get_error());
        }
        Ok(Self {
            _marker: PhantomData,
        })
    }

    /// Loads the Vulkan library. Pass `None` to use the system default.
    pub fn load_vulkan_library(&self, path: Option<&CStr>) -> Result<(), Error> {
        let ptr = path.map_or(std::ptr::null(), |p| p.as_ptr());
        // Safety: ptr is either null or a valid C string pointer.
        let ok = unsafe { vulkan::SDL_Vulkan_LoadLibrary(ptr) };
        if !ok {
            return Err(crate::get_error());
        }
        Ok(())
    }

    /// Creates a [`WindowBuilder`] with the given title and dimensions.
    pub fn window(&self, title: &str, width: u32, height: u32) -> WindowBuilder {
        WindowBuilder {
            title: CString::new(title).unwrap_or_default(),
            width: width as i32,
            height: height as i32,
            flags: ffi::SDL_WindowFlags(0),
            centered: false,
        }
    }
}

impl Drop for VideoSubsystem {
    fn drop(&mut self) {
        // Safety: Matches the SDL_InitSubSystem call in new().
        unsafe { init::SDL_QuitSubSystem(init::SDL_INIT_VIDEO) }
    }
}

/// Builder for creating an SDL3 window.
pub struct WindowBuilder {
    title: CString,
    width: i32,
    height: i32,
    flags: ffi::SDL_WindowFlags,
    centered: bool,
}

impl WindowBuilder {
    /// Enables high-DPI / high pixel density rendering.
    pub fn high_pixel_density(mut self) -> Self {
        self.flags.0 |= ffi::SDL_WINDOW_HIGH_PIXEL_DENSITY.0;
        self
    }

    /// Makes the window resizable.
    pub fn resizable(mut self) -> Self {
        self.flags.0 |= ffi::SDL_WINDOW_RESIZABLE.0;
        self
    }

    /// Centers the window on the display.
    pub fn position_centered(mut self) -> Self {
        self.centered = true;
        self
    }

    /// Creates the window initially hidden.
    pub fn hidden(mut self) -> Self {
        self.flags.0 |= ffi::SDL_WINDOW_HIDDEN.0;
        self
    }

    /// Creates the window in fullscreen (borderless desktop) mode.
    pub fn fullscreen(mut self) -> Self {
        self.flags.0 |= ffi::SDL_WINDOW_FULLSCREEN.0;
        self
    }

    /// Marks the window for use with Vulkan rendering.
    pub fn vulkan(mut self) -> Self {
        self.flags.0 |= ffi::SDL_WINDOW_VULKAN.0;
        self
    }

    /// Builds and returns the window.
    pub fn build(self) -> Result<Window, Error> {
        // Safety: We pass valid title, dimensions and flags.
        let ptr = unsafe {
            ffi::SDL_CreateWindow(self.title.as_ptr(), self.width, self.height, self.flags)
        };
        if ptr.is_null() {
            return Err(crate::get_error());
        }

        if self.centered {
            // Safety: Window pointer is valid; we just created it.
            unsafe {
                ffi::SDL_SetWindowPosition(
                    ptr,
                    ffi::SDL_WINDOWPOS_CENTERED,
                    ffi::SDL_WINDOWPOS_CENTERED,
                );
            }
        }

        Ok(Window {
            ptr,
            _marker: PhantomData,
        })
    }
}

/// An SDL3 window. Destroyed on drop.
pub struct Window {
    ptr: *mut ffi::SDL_Window,
    _marker: PhantomData<*mut ()>,
}

impl Window {
    /// Returns the raw SDL3 window pointer.
    pub fn raw(&self) -> *mut ffi::SDL_Window {
        self.ptr
    }

    /// Shows a previously hidden window.
    pub fn show(&mut self) {
        // Safety: Window pointer is valid.
        unsafe {
            ffi::SDL_ShowWindow(self.ptr);
        }
    }

    /// Returns the window's logical size in screen coordinates.
    pub fn size(&self) -> (u32, u32) {
        let mut w: i32 = 0;
        let mut h: i32 = 0;
        // Safety: Window pointer is valid; w and h are valid pointers.
        unsafe {
            ffi::SDL_GetWindowSize(self.ptr, &raw mut w, &raw mut h);
        }
        (w as u32, h as u32)
    }

    /// Returns the window's size in pixels (physical size on high-DPI displays).
    pub fn size_in_pixels(&self) -> (u32, u32) {
        let mut w: i32 = 0;
        let mut h: i32 = 0;
        // Safety: Window pointer is valid; w and h are valid pointers.
        unsafe {
            ffi::SDL_GetWindowSizeInPixels(self.ptr, &raw mut w, &raw mut h);
        }
        (w as u32, h as u32)
    }

    /// Returns the display scale factor for this window.
    pub fn display_scale(&self) -> f32 {
        // Safety: Window pointer is valid.
        unsafe { ffi::SDL_GetWindowDisplayScale(self.ptr) }
    }

    /// Locks the window's aspect ratio to the given value for both min and max.
    pub fn set_aspect_ratio(&self, ratio: f32) -> Result<(), Error> {
        // Safety: Window pointer is valid.
        let ok = unsafe { ffi::SDL_SetWindowAspectRatio(self.ptr, ratio, ratio) };
        if !ok {
            return Err(crate::get_error());
        }
        Ok(())
    }

    /// Enables or disables fullscreen (borderless desktop) mode on this window.
    pub fn set_fullscreen(&self, fullscreen: bool) -> Result<(), Error> {
        let ok = unsafe { ffi::SDL_SetWindowFullscreen(self.ptr, fullscreen) };
        if !ok {
            return Err(crate::get_error());
        }
        Ok(())
    }

    /// Sets the window title.
    pub fn set_title(&self, title: &str) {
        let c_title = CString::new(title).unwrap_or_default();
        // Safety: Window pointer is valid; c_title is a valid C string.
        unsafe {
            ffi::SDL_SetWindowTitle(self.ptr, c_title.as_ptr());
        }
    }

    /// Enables or disables relative mouse mode on this window.
    pub fn set_relative_mouse_mode(&self, enabled: bool) -> Result<(), Error> {
        // Safety: Window pointer is valid.
        let ok = unsafe { mouse_ffi::SDL_SetWindowRelativeMouseMode(self.ptr, enabled) };
        if !ok {
            return Err(crate::get_error());
        }
        Ok(())
    }

    /// Returns the Vulkan instance extensions required for surface creation.
    pub fn vulkan_instance_extensions(&self) -> Result<Vec<String>, Error> {
        let mut count: u32 = 0;
        // Safety: count is a valid pointer. SDL3 no longer requires a window for this.
        let names_ptr = unsafe { vulkan::SDL_Vulkan_GetInstanceExtensions(&raw mut count) };
        if names_ptr.is_null() {
            return Err(crate::get_error());
        }
        let mut extensions = Vec::with_capacity(count as usize);
        for i in 0..count as usize {
            // Safety: names_ptr points to an array of `count` valid C strings.
            let cstr = unsafe { CStr::from_ptr(*names_ptr.add(i)) };
            extensions.push(cstr.to_string_lossy().into_owned());
        }
        Ok(extensions)
    }

    /// Creates a Vulkan surface for this window.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `instance` is a valid Vulkan instance handle.
    pub unsafe fn vulkan_create_surface(
        &mut self,
        instance: VkInstance,
    ) -> Result<vulkan::VkSurfaceKHR, Error> {
        let mut surface = unsafe { std::mem::zeroed::<vulkan::VkSurfaceKHR>() };
        let ok = unsafe {
            vulkan::SDL_Vulkan_CreateSurface(self.ptr, instance, std::ptr::null(), &raw mut surface)
        };
        if !ok {
            return Err(crate::get_error());
        }
        Ok(surface)
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        // Safety: Window pointer is valid and we own it.
        unsafe { ffi::SDL_DestroyWindow(self.ptr) }
    }
}

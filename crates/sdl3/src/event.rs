use std::{marker::PhantomData, mem::MaybeUninit};

use sdl3_sys::events;

use crate::{
    Error,
    keyboard::{Mod, Scancode},
    mouse::MouseButton,
};

/// An SDL3 input or window event.
pub enum Event {
    /// The user requested to quit the application.
    Quit,
    /// A window event occurred.
    Window {
        /// The specific window event.
        win_event: WindowEvent,
    },
    /// A display event occurred.
    Display {
        /// The specific display event.
        display_event: DisplayEvent,
    },
    /// A key was pressed.
    KeyDown {
        /// The physical key scancode, if recognized.
        scancode: Option<Scancode>,
        /// Active keyboard modifiers.
        keymod: Mod,
        /// Whether this is a key-repeat event.
        repeat: bool,
    },
    /// A key was released.
    KeyUp {
        /// The physical key scancode, if recognized.
        scancode: Option<Scancode>,
        /// Whether this is a key-repeat event.
        repeat: bool,
    },
    /// The mouse was moved.
    MouseMotion {
        /// Relative X motion.
        xrel: f32,
        /// Relative Y motion.
        yrel: f32,
    },
    /// A mouse button was pressed.
    MouseButtonDown {
        /// The button that was pressed.
        mouse_btn: MouseButton,
    },
    /// A mouse button was released.
    MouseButtonUp {
        /// The button that was released.
        mouse_btn: MouseButton,
    },
    /// An event type not handled by this wrapper.
    Unknown,
}

/// A window-specific event.
pub enum WindowEvent {
    /// The window was resized to the given logical dimensions.
    Resized(i32, i32),
    /// The window's pixel size changed.
    PixelSizeChanged(i32, i32),
    /// The window lost keyboard focus.
    FocusLost,
    /// The window gained keyboard focus.
    FocusGained,
    /// A window event not handled by this wrapper.
    Other,
}

/// A display-specific event.
pub enum DisplayEvent {
    /// The display's content scale factor changed.
    ContentScaleChanged,
    /// A display event not handled by this wrapper.
    Other,
}

/// Polls SDL3 events. Obtained via [`crate::Sdl::event_pump`].
pub struct EventPump {
    _marker: PhantomData<*mut ()>,
}

impl EventPump {
    pub(crate) fn new() -> Result<Self, Error> {
        Ok(Self {
            _marker: PhantomData,
        })
    }

    /// Returns an iterator that drains all pending events.
    pub fn poll_iter(&mut self) -> EventPollIterator<'_> {
        EventPollIterator { _pump: self }
    }
}

/// Iterator over pending SDL3 events.
pub struct EventPollIterator<'a> {
    _pump: &'a mut EventPump,
}

impl Iterator for EventPollIterator<'_> {
    type Item = Event;

    fn next(&mut self) -> Option<Self::Item> {
        let mut raw = MaybeUninit::<events::SDL_Event>::uninit();
        // Safety: SDL_PollEvent writes into the provided pointer and returns
        // true when an event is available.
        let has_event = unsafe { events::SDL_PollEvent(raw.as_mut_ptr()) };
        if !has_event {
            return None;
        }
        // Safety: SDL_PollEvent returned true, so the event is initialized.
        let raw = unsafe { raw.assume_init() };
        Some(convert_event(&raw))
    }
}

fn convert_event(raw: &events::SDL_Event) -> Event {
    // Safety: The `type` field is always valid to read from the union.
    let event_type = events::SDL_EventType(unsafe { raw.r#type });

    match event_type {
        events::SDL_EVENT_QUIT => Event::Quit,

        events::SDL_EVENT_WINDOW_RESIZED => {
            // Safety: We checked the event type matches a window event.
            let w = unsafe { raw.window.data1 };
            let h = unsafe { raw.window.data2 };
            Event::Window {
                win_event: WindowEvent::Resized(w, h),
            }
        }

        events::SDL_EVENT_WINDOW_PIXEL_SIZE_CHANGED => {
            let w = unsafe { raw.window.data1 };
            let h = unsafe { raw.window.data2 };
            Event::Window {
                win_event: WindowEvent::PixelSizeChanged(w, h),
            }
        }

        events::SDL_EVENT_WINDOW_FOCUS_LOST => Event::Window {
            win_event: WindowEvent::FocusLost,
        },

        events::SDL_EVENT_WINDOW_FOCUS_GAINED => Event::Window {
            win_event: WindowEvent::FocusGained,
        },

        events::SDL_EVENT_DISPLAY_CONTENT_SCALE_CHANGED => Event::Display {
            display_event: DisplayEvent::ContentScaleChanged,
        },

        events::SDL_EVENT_KEY_DOWN => {
            let key = unsafe { &raw.key };
            Event::KeyDown {
                scancode: Scancode::from_raw(key.scancode),
                keymod: Mod(key.r#mod.0),
                repeat: key.repeat,
            }
        }

        events::SDL_EVENT_KEY_UP => {
            let key = unsafe { &raw.key };
            Event::KeyUp {
                scancode: Scancode::from_raw(key.scancode),
                repeat: key.repeat,
            }
        }

        events::SDL_EVENT_MOUSE_MOTION => {
            let motion = unsafe { &raw.motion };
            Event::MouseMotion {
                xrel: motion.xrel,
                yrel: motion.yrel,
            }
        }

        events::SDL_EVENT_MOUSE_BUTTON_DOWN => {
            let button = unsafe { &raw.button };
            Event::MouseButtonDown {
                mouse_btn: MouseButton::from_raw(button.button),
            }
        }

        events::SDL_EVENT_MOUSE_BUTTON_UP => {
            let button = unsafe { &raw.button };
            Event::MouseButtonUp {
                mouse_btn: MouseButton::from_raw(button.button),
            }
        }

        _ => Event::Unknown,
    }
}

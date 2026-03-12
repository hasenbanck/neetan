use std::{marker::PhantomData, mem::ManuallyDrop, ptr};

use sdl3_sys::{audio as ffi, init};

use crate::Error;

/// Manages the SDL3 audio subsystem. Calls `SDL_QuitSubSystem(AUDIO)` on drop.
pub struct AudioSubsystem {
    _marker: PhantomData<*mut ()>,
}

impl AudioSubsystem {
    pub(crate) fn new() -> Result<Self, Error> {
        // Safety: Called from the main thread after SDL_Init.
        let ok = unsafe { init::SDL_InitSubSystem(init::SDL_INIT_AUDIO) };
        if !ok {
            return Err(crate::get_error());
        }
        Ok(Self {
            _marker: PhantomData,
        })
    }

    /// Opens the default playback audio device with the given spec.
    pub fn open_playback_device(self, spec: &AudioSpec) -> Result<AudioDevice, Error> {
        let raw_spec = spec.to_raw();
        // Safety: We pass a valid spec pointer; SDL returns 0 on failure.
        let device_id =
            unsafe { ffi::SDL_OpenAudioDevice(ffi::SDL_AUDIO_DEVICE_DEFAULT_PLAYBACK, &raw_spec) };
        if device_id.0 == 0 {
            return Err(crate::get_error());
        }
        Ok(AudioDevice {
            device_id,
            _subsystem: self,
        })
    }
}

impl Drop for AudioSubsystem {
    fn drop(&mut self) {
        // Safety: Matches the SDL_InitSubSystem call in new().
        unsafe { init::SDL_QuitSubSystem(init::SDL_INIT_AUDIO) }
    }
}

/// An opened audio playback device. Owns the [`AudioSubsystem`] that created it.
pub struct AudioDevice {
    device_id: ffi::SDL_AudioDeviceID,
    _subsystem: AudioSubsystem,
}

impl AudioDevice {
    /// Opens a push-mode audio stream on this device, consuming the device.
    pub fn open_device_stream(self, spec: Option<&AudioSpec>) -> Result<AudioStreamOwner, Error> {
        let raw_spec;
        let spec_ptr = match spec {
            Some(s) => {
                raw_spec = s.to_raw();
                &raw_spec as *const ffi::SDL_AudioSpec
            }
            None => ptr::null(),
        };

        // Safety: device_id is valid. Passing null callback/userdata for push mode.
        let stream_ptr = unsafe {
            ffi::SDL_OpenAudioDeviceStream(self.device_id, spec_ptr, None, ptr::null_mut())
        };
        if stream_ptr.is_null() {
            return Err(crate::get_error());
        }

        // The stream now owns the device and subsystem; prevent their Drop impls from running.
        let this = ManuallyDrop::new(self);

        // Safety: We're reading the subsystem out of `this` before forgetting `this`.
        // The device_id is now owned by the SDL3 stream, so we must not close it.
        let subsystem = unsafe { ptr::read(&this._subsystem) };

        Ok(AudioStreamOwner {
            stream: stream_ptr,
            _subsystem: subsystem,
        })
    }
}

impl Drop for AudioDevice {
    fn drop(&mut self) {
        // Safety: device_id is valid and hasn't been transferred to a stream.
        unsafe { ffi::SDL_CloseAudioDevice(self.device_id) }
    }
}

/// Owns an SDL3 audio stream, its underlying device, and the audio subsystem.
///
/// Dropping this destroys the stream and device, then quits the audio subsystem.
pub struct AudioStreamOwner {
    stream: *mut ffi::SDL_AudioStream,
    _subsystem: AudioSubsystem,
}

impl AudioStreamOwner {
    /// Pushes `f32` sample data into the stream's queue.
    pub fn put_data_f32(&self, data: &[f32]) -> Result<(), Error> {
        let len = size_of_val(data) as i32;
        // Safety: data pointer and length are valid; stream pointer is valid.
        let ok = unsafe { ffi::SDL_PutAudioStreamData(self.stream, data.as_ptr().cast(), len) };
        if !ok {
            return Err(crate::get_error());
        }
        Ok(())
    }

    /// Resumes playback on the stream's device.
    pub fn resume(&self) -> Result<(), Error> {
        // Safety: stream pointer is valid.
        let ok = unsafe { ffi::SDL_ResumeAudioStreamDevice(self.stream) };
        if !ok {
            return Err(crate::get_error());
        }
        Ok(())
    }

    /// Pauses playback on the stream's device.
    pub fn pause(&self) -> Result<(), Error> {
        // Safety: stream pointer is valid.
        let ok = unsafe { ffi::SDL_PauseAudioStreamDevice(self.stream) };
        if !ok {
            return Err(crate::get_error());
        }
        Ok(())
    }

    /// Clears all queued data from the stream.
    pub fn clear(&self) -> Result<(), Error> {
        // Safety: stream pointer is valid.
        let ok = unsafe { ffi::SDL_ClearAudioStream(self.stream) };
        if !ok {
            return Err(crate::get_error());
        }
        Ok(())
    }

    /// Returns the number of bytes currently queued in the stream.
    pub fn queued_bytes(&self) -> Result<i32, Error> {
        // Safety: stream pointer is valid.
        let bytes = unsafe { ffi::SDL_GetAudioStreamQueued(self.stream) };
        if bytes < 0 {
            return Err(crate::get_error());
        }
        Ok(bytes)
    }
}

impl Drop for AudioStreamOwner {
    fn drop(&mut self) {
        // Safety: stream pointer is valid. Also closes the underlying device.
        unsafe { ffi::SDL_DestroyAudioStream(self.stream) }
    }
}

/// Describes the desired audio format, sample rate, and channel count.
pub struct AudioSpec {
    /// Sample rate in Hz, or `None` for the device default.
    pub freq: Option<i32>,
    /// Number of audio channels, or `None` for the device default.
    pub channels: Option<i32>,
    /// Sample format, or `None` for the device default.
    pub format: Option<AudioFormat>,
}

impl AudioSpec {
    fn to_raw(&self) -> ffi::SDL_AudioSpec {
        ffi::SDL_AudioSpec {
            format: self.format.map(|f| f.0).unwrap_or(ffi::SDL_AudioFormat(0)),
            channels: self.channels.unwrap_or(0),
            freq: self.freq.unwrap_or(0),
        }
    }
}

/// An audio sample format.
#[derive(Copy, Clone)]
pub struct AudioFormat(ffi::SDL_AudioFormat);

impl AudioFormat {
    /// 32-bit floating-point samples.
    pub fn f32_sys() -> Self {
        Self(ffi::SDL_AUDIO_F32)
    }
}

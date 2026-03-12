//! # Audio resampling library
//!
//! Resampler is a small, zero-dependency crate for high-quality audio resampling.
//!
//! ## Usage Examples
//!
//! ### FIR-Based Resampler (Low Latency, Streaming)
//!
//! ```rust
//! use resampler::{Attenuation, Latency, ResamplerFir};
//!
//! // Create a stereo resampler with configurable latency (16, 32, or 64 samples).
//! let mut resampler = ResamplerFir::new(2, 48000, 44100, Latency::Sample64, Attenuation::Db90);
//!
//! // Streaming API - accepts arbitrary input buffer sizes.
//! let input = vec![0.0f32; 512];
//! let mut output = vec![0.0f32; resampler.buffer_size_output()];
//!
//! let (consumed, produced) = resampler.resample(&input, &mut output).unwrap();
//! println!("Consumed {consumed} samples, produced {produced} samples");
//! ```
#![forbid(missing_docs)]

extern crate alloc;

mod error;
mod fir;
mod resampler_fir;
mod window;

pub use error::ResampleError;
pub use resampler_fir::{Attenuation, Latency, ResamplerFir};

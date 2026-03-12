use alloc::{boxed::Box, sync::Arc, vec, vec::Vec};
use core::{fmt, ops::Deref, ptr, slice};
use std::{
    alloc::{Layout, alloc, dealloc},
    collections::HashMap,
    sync::{LazyLock, Mutex},
};

use crate::{
    ResampleError,
    window::{calculate_cutoff_kaiser, make_sincs_for_kaiser},
};

const PHASES: usize = 1024;
const INPUT_CAPACITY: usize = 4096;
const BUFFER_SIZE: usize = INPUT_CAPACITY * 2;

type ConvolveFn =
    fn(input: &[f32], coeffs1: &[f32], coeffs2: &[f32], frac: f32, taps: usize) -> f32;

/// A 64-byte aligned memory of f32 values.
pub(crate) struct AlignedMemory {
    ptr: *mut f32,
    len: usize,
    layout: Layout,
}

impl AlignedMemory {
    pub(crate) fn new(data: Vec<f32>) -> Self {
        const ALIGNMENT: usize = 64;

        let len = data.len();
        let size = len * size_of::<f32>();

        unsafe {
            let layout = Layout::from_size_align(size, ALIGNMENT).expect("invalid layout");
            let ptr = alloc(layout) as *mut f32;

            if ptr.is_null() {
                panic!("failed to allocate aligned memory for FIR coefficients");
            }

            ptr::copy_nonoverlapping(data.as_ptr(), ptr, len);

            Self { ptr, len, layout }
        }
    }
}

impl Deref for AlignedMemory {
    type Target = [f32];

    fn deref(&self) -> &Self::Target {
        unsafe { slice::from_raw_parts(self.ptr, self.len) }
    }
}

impl Drop for AlignedMemory {
    fn drop(&mut self) {
        unsafe {
            dealloc(self.ptr as *mut u8, self.layout);
        }
    }
}

// Safety: AlignedSlice can be safely sent between threads.
unsafe impl Send for AlignedMemory {}

// Safety: AlignedSlice can be safely shared between threads (immutable access).
unsafe impl Sync for AlignedMemory {}

struct FirCacheData {
    coeffs: Arc<AlignedMemory>,
    taps: usize,
}

impl Clone for FirCacheData {
    fn clone(&self) -> Self {
        Self {
            coeffs: Arc::clone(&self.coeffs),
            taps: self.taps,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
struct FirCacheKey {
    cutoff_bits: u32,
    taps: usize,
    attenuation: Attenuation,
}

/// The desired stopband attenuation of the filter. Higher attenuation provides better stopband
/// rejection but slightly wider transition bands.
///
/// Defaults to -120 dB of stopband attenuation.
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum Attenuation {
    /// Stopband attenuation of around -60 dB (Inaudible threshold).
    Db60,
    /// Stopband attenuation of around -90 dB (transparent for 16-bit audio).
    Db90,
    /// Stopband attenuation of around -120 dB (transparent for 24-bit audio).
    #[default]
    Db120,
}

impl Attenuation {
    /// Returns the Kaiser window beta value for the desired attenuation level.
    ///
    /// The beta value controls the shape of the Kaiser window and directly affects
    /// the stopband attenuation of the resulting filter.
    pub(crate) fn to_kaiser_beta(self) -> f64 {
        match self {
            Attenuation::Db60 => 7.0,
            Attenuation::Db90 => 10.0,
            Attenuation::Db120 => 13.0,
        }
    }
}

/// Latency configuration for the FIR resampler.
///
/// Determines the number of filter taps, which affects both rolloff and algorithmic delay.
/// Higher tap counts provide shaper rolloff but increased latency.
///
/// The enum variants are named by their algorithmic delay in samples (taps / 2):
/// - `Sample8`: 8 samples delay (16 taps)
/// - `Sample16`: 16 samples delay (32 taps)
/// - `Sample32`: 32 samples delay (64 taps)
/// - `Sample64`: 64 samples delay (128 taps)
///
/// Defaults to 64 samples delay (128 taps).
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum Latency {
    /// 8 samples algorithmic delay (16 taps).
    Sample8,
    /// 16 samples algorithmic delay (32 taps).
    Sample16,
    /// 32 samples algorithmic delay (64 taps).
    Sample32,
    /// 64 samples algorithmic delay (128 taps).
    #[default]
    Sample64,
}

impl Latency {
    /// Returns the number of filter taps for this latency setting.
    pub const fn taps(self) -> usize {
        // Taps need to be a power of two for convolve filter to run (there is no tail handling).
        match self {
            Latency::Sample8 => 16,
            Latency::Sample16 => 32,
            Latency::Sample32 => 64,
            Latency::Sample64 => 128,
        }
    }
}

static FIR_CACHE: LazyLock<Mutex<HashMap<FirCacheKey, FirCacheData>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// High-quality polyphase FIR audio resampler supporting multi-channel audio with streaming API.
///
/// `ResamplerFir` uses a configurable polyphase FIR filter (32, 64, or 128 taps) decomposed
/// into 1024 branches for high-quality audio resampling with configurable latency.
/// The const generic parameter `CHANNEL` specifies the number of audio channels.
///
/// Unlike the FFT-based resampler, this implementation supports streaming with arbitrary
/// input buffer sizes, making it ideal for real-time applications. The latency can be
/// configured at construction time using the [`Latency`] enum to balance quality versus delay.
///
/// The stopband attenuation can also be configured via the [`Attenuation`] enum.
pub struct ResamplerFir {
    /// Number of audio channels.
    channels: usize,
    /// Polyphase coefficient table stored contiguously: all phases × taps in a single allocation.
    /// Layout: [phase0_tap0..N, phase1_tap0..N, ..., phase1023_tap0..N]
    coeffs: Arc<AlignedMemory>,
    /// Per-channel double-sized input buffers for efficient buffer management.
    /// Size = BUFFER_SIZE (2x INPUT_CAPACITY) to minimize copy operations.
    input_buffers: Box<[f32]>,
    /// Read position in the input buffer (where we start reading from).
    read_position: usize,
    /// Number of valid frames available for processing (from read_position).
    available_frames: usize,
    /// Current fractional position within available frames.
    position: f64,
    /// Resampling ratio (input_rate / output_rate).
    ratio: f64,
    /// Number of taps per phase.
    taps: usize,
    /// Number of polyphase branches.
    phases: usize,
    convolve_function: ConvolveFn,
}

impl fmt::Debug for ResamplerFir {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResamplerFir")
            .field("channels", &self.channels)
            .field("taps", &self.taps)
            .field("phases", &self.phases)
            .finish_non_exhaustive()
    }
}

impl ResamplerFir {
    /// Create a new [`ResamplerFir`].
    ///
    /// Parameters:
    /// - `channels`: The channel count.
    /// - `input_rate`: Input sample rate.
    /// - `output_rate`: Output sample rate.
    /// - `latency`: Latency configuration determining filter length (32, 64, or 128 taps).
    /// - `attenuation`: Desired stopband attenuation controlling filter quality.
    ///
    /// The resampler will generate polyphase filter coefficients optimized for the
    /// given sample rate pair, using a Kaiser window with beta value determined by the
    /// attenuation setting. Higher tap counts provide better frequency response at the
    /// cost of increased latency. Higher attenuation provides better stopband rejection
    /// but slightly wider transition bands.
    ///
    /// # Example
    ///
    /// ```rust
    /// use resampler::{Attenuation, Latency, ResamplerFir};
    ///
    /// // Create with default latency (128 taps, 64 samples delay) and 90 dB attenuation
    /// let resampler = ResamplerFir::new(2, 48000, 44100, Latency::default(), Attenuation::default());
    ///
    /// // Create with low latency (32 taps, 16 samples delay) and 60 dB attenuation
    /// let resampler_low_latency =
    ///     ResamplerFir::new(2, 48000, 44100, Latency::Sample16, Attenuation::Db60);
    /// ```
    pub fn new(
        channels: usize,
        input_rate: u32,
        output_rate: u32,
        latency: Latency,
        attenuation: Attenuation,
    ) -> Self {
        let input_rate_hz = f64::from(input_rate);
        let output_rate_hz = f64::from(output_rate);
        let ratio = input_rate_hz / output_rate_hz;

        let taps = latency.taps();
        let beta = attenuation.to_kaiser_beta();
        let base_cutoff = calculate_cutoff_kaiser(taps, beta);
        let cutoff = if input_rate_hz <= output_rate_hz {
            // Upsampling: preserve full input bandwidth.
            base_cutoff
        } else {
            // Downsampling: scale cutoff to output Nyquist (anti-aliasing filter).
            base_cutoff * (output_rate_hz / input_rate_hz)
        };

        let coeffs = Self::get_or_create_fir_coeffs(cutoff as f32, taps, attenuation);

        // Allocate double-sized buffers for efficient buffer management.
        let input_buffers = vec![0.0; BUFFER_SIZE * channels].into_boxed_slice();

        #[cfg(target_arch = "x86_64")]
        let convolve_function = if std::arch::is_x86_feature_detected!("avx512f") && taps >= 16 {
            fn wrapper(
                input: &[f32],
                coeffs1: &[f32],
                coeffs2: &[f32],
                frac: f32,
                taps: usize,
            ) -> f32 {
                unsafe {
                    crate::fir::avx512::convolve_interp_avx512(input, coeffs1, coeffs2, frac, taps)
                }
            }
            wrapper
        } else if std::arch::is_x86_feature_detected!("avx")
            && std::arch::is_x86_feature_detected!("fma")
        {
            fn wrapper(
                input: &[f32],
                coeffs1: &[f32],
                coeffs2: &[f32],
                frac: f32,
                taps: usize,
            ) -> f32 {
                unsafe {
                    crate::fir::avx::convolve_interp_avx_fma(input, coeffs1, coeffs2, frac, taps)
                }
            }
            wrapper
        } else if std::arch::is_x86_feature_detected!("sse4.2") {
            fn wrapper(
                input: &[f32],
                coeffs1: &[f32],
                coeffs2: &[f32],
                frac: f32,
                taps: usize,
            ) -> f32 {
                unsafe {
                    crate::fir::sse4_2::convolve_interp_sse4_2(input, coeffs1, coeffs2, frac, taps)
                }
            }
            wrapper
        } else {
            // SSE2 is always available.
            fn wrapper(
                input: &[f32],
                coeffs1: &[f32],
                coeffs2: &[f32],
                frac: f32,
                taps: usize,
            ) -> f32 {
                unsafe {
                    crate::fir::sse2::convolve_interp_sse2(input, coeffs1, coeffs2, frac, taps)
                }
            }
            wrapper
        };

        ResamplerFir {
            channels,
            coeffs,
            input_buffers,
            read_position: 0,
            available_frames: 0,
            position: 0.0,
            ratio,
            taps,
            phases: PHASES,
            #[cfg(target_arch = "x86_64")]
            convolve_function,
            #[cfg(not(target_arch = "x86_64"))]
            convolve_function: crate::fir::convolve_interp,
        }
    }

    fn create_fir_coeffs(cutoff: f32, taps: usize, beta: f64) -> FirCacheData {
        let polyphase_coeffs = make_sincs_for_kaiser(taps, PHASES, cutoff, beta);

        // Flatten the polyphase coefficients into a single contiguous allocation.
        // Layout: [phase0_tap0..N, phase1_tap0..N, ..., phase1023_tap0..N]
        let total_size = PHASES * taps;
        let mut flattened = Vec::with_capacity(total_size);
        for phase_coeffs in polyphase_coeffs {
            flattened.extend_from_slice(&phase_coeffs);
        }

        FirCacheData {
            coeffs: Arc::new(AlignedMemory::new(flattened)),
            taps,
        }
    }

    fn get_or_create_fir_coeffs(
        cutoff: f32,
        taps: usize,
        attenuation: Attenuation,
    ) -> Arc<AlignedMemory> {
        let cache_key = FirCacheKey {
            cutoff_bits: cutoff.to_bits(),
            taps,
            attenuation,
        };
        let beta = attenuation.to_kaiser_beta();
        FIR_CACHE
            .lock()
            .unwrap()
            .entry(cache_key)
            .or_insert_with(|| Self::create_fir_coeffs(cutoff, taps, beta))
            .clone()
            .coeffs
    }

    /// Calculate the maximum output buffer size that needs to be allocated.
    pub fn buffer_size_output(&self) -> usize {
        // Conservative upper bound: assume buffer could be maximally filled.
        let max_total_frames = INPUT_CAPACITY;
        let max_usable_frames = (max_total_frames - self.taps) as f64;
        let max_output_frames = (max_usable_frames / self.ratio).ceil() as usize + 2;
        max_output_frames * self.channels
    }

    /// Process audio samples, resampling from input to output sample rate.
    ///
    /// This is a streaming API that accepts arbitrary input buffer sizes and produces
    /// as many output samples as possible given the available input.
    ///
    /// Input and output must be interleaved f32 slices with all channels interleaved.
    /// For stereo audio, the format is `[L0, R0, L1, R1, ...]`. For mono, it's `[S0, S1, S2, ...]`.
    ///
    /// ## Parameters
    ///
    /// - `input`: Interleaved input samples. Length must be a multiple of `CHANNEL`.
    /// - `output`: Interleaved output buffer. Length must be a multiple of `CHANNEL`.
    ///
    /// ## Returns
    ///
    /// `Ok((consumed, produced))` where:
    /// - `consumed`: Number of input samples consumed (in total f32 values, including all channels).
    /// - `produced`: Number of output samples produced (in total f32 values, including all channels).
    ///
    /// ## Example
    ///
    /// ```rust
    /// use resampler::{Attenuation, Latency, ResamplerFir};
    ///
    /// let mut resampler =
    ///     ResamplerFir::new(1, 48000, 44100, Latency::default(), Attenuation::default());
    /// let buffer_size_output = resampler.buffer_size_output();
    /// let input = vec![0.0f32; 256];
    /// let mut output = vec![0.0f32; buffer_size_output];
    ///
    /// match resampler.resample(&input, &mut output) {
    ///     Ok((consumed, produced)) => {
    ///         println!("Processed {consumed} input samples into {produced} output samples");
    ///     }
    ///     Err(error) => eprintln!("Resampling error: {error:?}"),
    /// }
    /// ```
    pub fn resample(
        &mut self,
        input: &[f32],
        output: &mut [f32],
    ) -> Result<(usize, usize), ResampleError> {
        if !input.len().is_multiple_of(self.channels) {
            return Err(ResampleError::InvalidInputBufferSize);
        }
        if !output.len().is_multiple_of(self.channels) {
            return Err(ResampleError::InvalidOutputBufferSize);
        }

        let input_frames = input.len() / self.channels;
        let output_capacity = output.len() / self.channels;

        let write_position = self.read_position + self.available_frames;
        let remaining_capacity = BUFFER_SIZE.saturating_sub(write_position);
        let frames_to_copy = input_frames
            .min(remaining_capacity)
            .min(INPUT_CAPACITY - self.available_frames);

        // Deinterleave and copy input frames into double-sized buffers.
        for frame_idx in 0..frames_to_copy {
            for channel in 0..self.channels {
                let channel_buf = &mut self.input_buffers[BUFFER_SIZE * channel..];
                channel_buf[write_position + frame_idx] =
                    input[frame_idx * self.channels + channel];
            }
        }
        self.available_frames += frames_to_copy;

        let mut output_frame_count = 0;

        loop {
            let input_offset = self.position.floor() as usize;

            // Check if we have enough input samples (need `taps` samples for convolution).
            if input_offset + self.taps > self.available_frames {
                break;
            }

            if output_frame_count >= output_capacity {
                break;
            }

            let position_fract = self.position.fract();
            let phase_f = (position_fract * self.phases as f64).min((self.phases - 1) as f64);
            let phase1 = phase_f as usize;
            let phase2 = (phase1 + 1).min(self.phases - 1);
            let frac = (phase_f - phase1 as f64) as f32;

            for channel in 0..self.channels {
                // Perform N-tap convolution with linear interpolation between phases.
                let actual_pos = self.read_position + input_offset;
                let channel_buf = &self.input_buffers[BUFFER_SIZE * channel..];
                let input_slice = &channel_buf[actual_pos..actual_pos + self.taps];

                let phase1_start = phase1 * self.taps;
                let coeffs_phase1 = &self.coeffs[phase1_start..phase1_start + self.taps];
                let phase2_start = phase2 * self.taps;
                let coeffs_phase2 = &self.coeffs[phase2_start..phase2_start + self.taps];

                let sample = (self.convolve_function)(
                    input_slice,
                    coeffs_phase1,
                    coeffs_phase2,
                    frac,
                    self.taps,
                );
                output[output_frame_count * self.channels + channel] = sample;
            }

            output_frame_count += 1;
            self.position += self.ratio;
        }

        // Update buffer state: consume processed frames.

        let consumed_frames = self.position.floor() as usize;

        self.read_position += consumed_frames;
        self.available_frames -= consumed_frames;
        self.position -= consumed_frames as f64;

        // Double-buffer optimization: only copy when read_position exceeds threshold.
        if self.read_position > INPUT_CAPACITY {
            // Copy remaining valid data to the beginning of the buffer.
            for channel in 0..self.channels {
                let channel_buf = &mut self.input_buffers[BUFFER_SIZE * channel..];
                channel_buf.copy_within(
                    self.read_position..self.read_position + self.available_frames,
                    0,
                );
            }
            self.read_position = 0;
        }

        Ok((
            frames_to_copy * self.channels,
            output_frame_count * self.channels,
        ))
    }

    /// Returns the algorithmic delay (latency) of the resampler in input samples.
    ///
    /// For the polyphase FIR resampler, this equals half the filter length due to the
    /// symmetric FIR filter design:
    /// - `Latency::_16`: 16 samples (32 taps / 2)
    /// - `Latency::_32`: 32 samples (64 taps / 2)
    /// - `Latency::_64`: 64 samples (128 taps / 2)
    pub fn delay(&self) -> usize {
        self.taps / 2
    }

    /// Resets the resampler state, clearing all internal buffers.
    ///
    /// Call this when starting to process a new audio stream to avoid
    /// discontinuities from previous audio data.
    pub fn reset(&mut self) {
        self.read_position = 0;
        self.available_frames = 0;
        self.position = 0.0;
    }
}

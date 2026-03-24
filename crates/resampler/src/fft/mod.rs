pub(crate) mod butterflies;
pub(crate) mod optimizer;
pub(crate) mod planner;
mod radix_fft;
mod real_complex;
pub(crate) mod stockham_autosort;

pub(crate) use radix_fft::Inverse;
pub use radix_fft::{Forward, Radix, RadixFFT};

/// Simple complex number struct
#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct Complex32 {
    /// Real component.
    pub re: f32,
    /// Imaginary component.
    pub im: f32,
}

impl Complex32 {
    #[inline(always)]
    /// Creates a new complex number.
    pub const fn new(re: f32, im: f32) -> Self {
        Self { re, im }
    }

    #[inline(always)]
    pub(crate) const fn zero() -> Self {
        Self { re: 0.0, im: 0.0 }
    }

    #[inline(always)]
    pub(crate) const fn conj(&self) -> Self {
        Self {
            re: self.re,
            im: -self.im,
        }
    }

    #[inline(always)]
    pub(crate) const fn add(&self, o: &Self) -> Self {
        Self {
            re: self.re + o.re,
            im: self.im + o.im,
        }
    }

    #[inline(always)]
    pub(crate) const fn sub(&self, o: &Self) -> Self {
        Self {
            re: self.re - o.re,
            im: self.im - o.im,
        }
    }

    #[inline(always)]
    pub(crate) const fn mul(&self, o: &Self) -> Self {
        Self {
            re: self.re * o.re - self.im * o.im,
            im: self.re * o.im + self.im * o.re,
        }
    }

    #[inline(always)]
    #[cfg(test)]
    pub(crate) const fn scale(&self, f: f32) -> Self {
        Self {
            re: self.re * f,
            im: self.im * f,
        }
    }
}

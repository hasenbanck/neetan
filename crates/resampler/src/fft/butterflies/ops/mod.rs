#[cfg(target_arch = "aarch64")]
mod neon;

#[cfg(target_arch = "aarch64")]
pub(crate) use neon::*;

#[cfg(target_arch = "x86_64")]
mod sse2;

#[cfg(target_arch = "x86_64")]
pub(crate) use sse2::*;

#[cfg(target_arch = "x86_64")]
mod sse4_2;

#[cfg(target_arch = "x86_64")]
pub(crate) use sse4_2::*;

#[cfg(target_arch = "x86_64")]
mod avx;

#[cfg(target_arch = "x86_64")]
pub(crate) use avx::*;

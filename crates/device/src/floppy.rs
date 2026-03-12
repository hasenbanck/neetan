//! Floppy disk image format parsers for FDC emulation.
//!
//! Supports three PC-98 floppy image formats:
//! - **D88** (.d88/.d98/.88d/.98d): Standard D88 format with per-sector metadata.
//! - **HDM** (.hdm): Headerless raw sector format for 2HD floppies.
//! - **NFD** (.nfd): T98Next format with per-sector metadata (R0 and R1 revisions).

pub mod d88;
pub mod hdm;
pub mod nfd;

use std::{
    error::Error,
    fmt,
    ops::{Deref, DerefMut},
    path::Path,
};

pub use d88::{D88Disk, D88Error, D88MediaType, D88Sector};

/// The original format of a loaded floppy image (used for serialization).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloppyFormat {
    /// Standard D88 format (.d88/.d98/.88d/.98d).
    D88,
    /// Headerless raw sector format (.hdm).
    Hdm,
    /// T98Next floppy format (.nfd).
    Nfd,
}

/// A parsed floppy disk image.
#[derive(Debug, Clone)]
pub struct FloppyImage {
    /// The underlying D88 disk data (canonical internal representation).
    disk: D88Disk,
    /// Original image format.
    pub format: FloppyFormat,
}

impl Deref for FloppyImage {
    type Target = D88Disk;
    fn deref(&self) -> &Self::Target {
        &self.disk
    }
}

impl DerefMut for FloppyImage {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.disk
    }
}

impl FloppyImage {
    /// Creates a `FloppyImage` from a pre-parsed `D88Disk`.
    pub fn from_d88(disk: D88Disk) -> Self {
        Self {
            disk,
            format: FloppyFormat::D88,
        }
    }

    /// Parses a D88 floppy image from raw bytes.
    pub fn from_d88_bytes(data: &[u8]) -> Result<Self, FloppyError> {
        let disk = D88Disk::from_bytes(data).map_err(FloppyError::D88)?;
        Ok(Self {
            disk,
            format: FloppyFormat::D88,
        })
    }

    /// Parses an HDM floppy image from raw bytes.
    pub fn from_hdm_bytes(data: &[u8]) -> Result<Self, FloppyError> {
        let disk = hdm::from_bytes(data).map_err(FloppyError::Hdm)?;
        Ok(Self {
            disk,
            format: FloppyFormat::Hdm,
        })
    }

    /// Parses an NFD floppy image from raw bytes.
    pub fn from_nfd_bytes(data: &[u8]) -> Result<Self, FloppyError> {
        let disk = nfd::from_bytes(data).map_err(FloppyError::Nfd)?;
        Ok(Self {
            disk,
            format: FloppyFormat::Nfd,
        })
    }

    /// Returns a human-readable format name.
    pub fn format_name(&self) -> &'static str {
        match self.format {
            FloppyFormat::D88 => "D88",
            FloppyFormat::Hdm => "HDM",
            FloppyFormat::Nfd => "NFD",
        }
    }

    /// Returns whether this image can be written back to disk.
    /// Only D88 format supports lossless roundtrip serialization.
    pub fn can_write_back(&self) -> bool {
        matches!(self.format, FloppyFormat::D88)
    }

    /// Serializes the image back to D88 format.
    pub fn to_bytes(&self) -> Vec<u8> {
        self.disk.to_bytes()
    }
}

/// Loads a floppy image, auto-detecting the format by file extension.
pub fn load_floppy_image(path: &Path, data: &[u8]) -> Result<FloppyImage, FloppyError> {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());

    match extension.as_deref() {
        Some("hdm") => FloppyImage::from_hdm_bytes(data),
        Some("nfd") => FloppyImage::from_nfd_bytes(data),
        Some("d88") | Some("d98") | Some("88d") | Some("98d") => FloppyImage::from_d88_bytes(data),
        _ => FloppyImage::from_d88_bytes(data),
    }
}

/// Error type for floppy image parsing.
#[derive(Debug, Clone)]
pub enum FloppyError {
    /// D88 format parsing error.
    D88(D88Error),
    /// HDM format parsing error.
    Hdm(hdm::HdmError),
    /// NFD format parsing error.
    Nfd(nfd::NfdError),
    /// File extension not recognized as a supported floppy format.
    UnrecognizedFormat,
}

impl fmt::Display for FloppyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FloppyError::D88(err) => write!(f, "{err}"),
            FloppyError::Hdm(err) => write!(f, "{err}"),
            FloppyError::Nfd(err) => write!(f, "{err}"),
            FloppyError::UnrecognizedFormat => write!(f, "unrecognized floppy image format"),
        }
    }
}

impl Error for FloppyError {}

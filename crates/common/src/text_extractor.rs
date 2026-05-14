//! Sink trait for extracted text.
//!
//! A [`TextExtractor`] receives one JIS code at a time as the emulator
//! observes glyphs and is responsible for grouping them into lines and
//! emitting them to whatever output sink the implementation chooses
//! (clipboard, file, network, etc.).

use crate::JisChar;

/// Receiver for glyphs as they are fetched by the emulator.
pub trait TextExtractor {
    /// Pushes one JIS code.
    ///
    /// Implementations decide when to flush the accumulated buffer; they
    /// may also drop codes that do not map to any printable Unicode.
    fn push_jis(&mut self, code: JisChar);

    /// Heartbeat called once per host frame from the main event loop.
    ///
    /// Lets time-based extractors flush a stale buffer even when no new
    /// glyph fetches are happening (e.g. the player is paused on a
    /// dialog box).
    fn tick(&mut self);
}

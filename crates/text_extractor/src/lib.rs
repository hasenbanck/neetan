//! Text extractor implementations for the PC-98 emulator.
//!
//! Implements [`common::TextExtractor`] sinks that consume JIS codes
//! observed at the CGROM glyph fetch port and emit completed lines to
//! external destinations.

#![warn(missing_docs)]
#![deny(unsafe_code)]

use std::time::{Duration, Instant};

use common::{JisChar, TextExtractor, jis_to_char, warn};

/// Idle window after which the accumulated buffer is treated as a
/// complete line and flushed to the clipboard.
const IDLE_THRESHOLD: Duration = Duration::from_millis(500);

/// Hard cap on the buffered character count, to bound memory and to
/// force a flush when a runaway sequence is being captured (e.g. a
/// scrolling credits screen).
const MAX_BUFFER: usize = 256;

/// Window during which a repeat push of the same JIS code is treated as
/// part of the same glyph render (16 scanlines x 2 lr halves at port
/// 0xA9) and dropped. Distinct from `IDLE_THRESHOLD`: repeated reads of
/// one glyph are not new activity for idle-flush purposes.
const DEDUPE_WINDOW: Duration = Duration::from_millis(50);

/// Time-based, clipboard-output text extractor.
///
/// Accumulates JIS codes in an internal buffer; flushes the buffer to the
/// system clipboard as one UTF-8 string when either:
///
/// - more than `IDLE_THRESHOLD` has elapsed since the last push (the
///   line is considered complete), or
/// - the buffer reaches `MAX_BUFFER` characters.
pub struct ClipboardExtractor {
    buffer: String,
    last_push: Option<Instant>,
    last_pushed_code: Option<JisChar>,
    last_pushed_at: Option<Instant>,
}

impl ClipboardExtractor {
    /// Creates a new clipboard extractor with an empty buffer.
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            last_push: None,
            last_pushed_code: None,
            last_pushed_at: None,
        }
    }

    fn flush(&mut self) {
        // Per-glyph dedupe state (`last_pushed_code` / `last_pushed_at`) is
        // intentionally NOT reset here: it tracks the most recent JIS code
        // observed at the cgrom port and is independent of the line buffer
        // lifecycle. Resetting it would let the trailing scanline-sweep
        // reads of the just-flushed glyph slip through.
        if self.buffer.is_empty() {
            self.last_push = None;
            return;
        }
        if let Err(error) = sdl3::clipboard::set_text(&self.buffer) {
            warn!("text extractor: clipboard write failed: {error}");
        }
        self.buffer.clear();
        self.last_push = None;
    }
}

impl Default for ClipboardExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl TextExtractor for ClipboardExtractor {
    fn push_jis(&mut self, code: JisChar) {
        let now = Instant::now();
        if self.last_pushed_code == Some(code)
            && let Some(previous) = self.last_pushed_at
            && now.duration_since(previous) < DEDUPE_WINDOW
        {
            self.last_pushed_at = Some(now);
            return;
        }
        self.last_pushed_code = Some(code);
        self.last_pushed_at = Some(now);

        if let Some(previous) = self.last_push
            && now.duration_since(previous) > IDLE_THRESHOLD
        {
            self.flush();
        }
        if let Some(character) = jis_to_char(code) {
            self.buffer.push(character);
        }
        self.last_push = Some(now);
        if self.buffer.chars().count() >= MAX_BUFFER {
            self.flush();
        }
    }

    fn tick(&mut self) {
        if let Some(previous) = self.last_push
            && Instant::now().duration_since(previous) > IDLE_THRESHOLD
            && !self.buffer.is_empty()
        {
            self.flush();
        }
    }
}

#[cfg(test)]
mod tests {
    use common::{JisChar, TextExtractor};

    use super::ClipboardExtractor;

    #[test]
    fn dedupes_repeated_pushes_of_same_glyph() {
        let mut extractor = ClipboardExtractor::new();

        let glyph_a = JisChar::from_u16(0x2121);
        for _ in 0..32 {
            extractor.push_jis(glyph_a);
        }

        let glyph_b = JisChar::from_u16(0x2122);
        for _ in 0..16 {
            extractor.push_jis(glyph_b);
        }

        assert_eq!(
            extractor.buffer.chars().count(),
            2,
            "expected one buffer entry per distinct glyph, got: {:?}",
            extractor.buffer
        );
    }
}

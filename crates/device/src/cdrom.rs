//! CD-ROM disc image abstraction.
//!
//! Supports CUE/BIN disc images. The CUE sheet describes the disc layout
//! (tracks, indices, sector sizes) while the BIN file contains raw sector data.
//!
//! Sector sizes:
//! - 2048 bytes: cooked data (user data only, typical for MODE1/2048)
//! - 2352 bytes: raw sector (sync + header + user data + EDC/ECC for data;
//!   raw audio for CDDA tracks, typical for MODE1/2352 and AUDIO)

use std::fmt;

/// CD-ROM sector sizes.
const SECTOR_SIZE_COOKED: u16 = 2048;
const SECTOR_SIZE_RAW: u16 = 2352;

/// Offset of user data within a raw Mode 1 sector (12-byte sync + 4-byte header).
const RAW_MODE1_DATA_OFFSET: usize = 16;

/// Offset of user data within a raw Mode 2/XA sector.
const RAW_MODE2_DATA_OFFSET: usize = 24;

/// Number of frames (sectors) per second of CD audio.
const FRAMES_PER_SECOND: u32 = 75;

/// Number of seconds per minute.
const SECONDS_PER_MINUTE: u32 = 60;

/// Track type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackType {
    /// Data track (Mode 1 CD-ROM).
    Data,
    /// Audio track (CD-DA).
    Audio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SectorLayout {
    Cooked,
    RawMode1,
    RawMode2,
    Audio,
}

/// A single track on the disc.
#[derive(Debug, Clone)]
pub struct Track {
    /// 1-based track number.
    pub number: u8,
    /// Track type (data or audio).
    pub track_type: TrackType,
    /// Sector size in the BIN file (2048 or 2352).
    pub sector_size: u16,
    /// LBA of INDEX 01 (track start).
    pub start_lba: u32,
    /// LBA of INDEX 00 (pregap start), or same as start_lba if no pregap in image.
    pub pregap_lba: u32,
    /// Number of sectors in this track (excluding pregap).
    pub sector_count: u32,
    /// Byte offset into the BIN file where this track's first sector begins.
    pub file_offset: u64,
    sector_layout: SectorLayout,
}

/// A loaded CD-ROM disc image.
#[derive(Debug, Clone)]
pub struct CdImage {
    tracks: Vec<Track>,
    data: Vec<u8>,
    total_sectors: u32,
}

/// Errors that can occur when parsing or reading CD-ROM images.
#[derive(Debug, Clone)]
pub enum CdError {
    /// CUE sheet parsing error.
    ParseError(String),
    /// Unsupported format or feature.
    UnsupportedFormat(String),
    /// Data size does not match expected layout.
    DataSizeMismatch {
        /// Expected size in bytes.
        expected: u64,
        /// Actual size in bytes.
        actual: u64,
    },
}

impl fmt::Display for CdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CdError::ParseError(msg) => write!(f, "CUE parse error: {msg}"),
            CdError::UnsupportedFormat(msg) => write!(f, "unsupported format: {msg}"),
            CdError::DataSizeMismatch { expected, actual } => {
                write!(
                    f,
                    "data size mismatch: expected {expected} bytes, got {actual}"
                )
            }
        }
    }
}

/// Converts a MSF (minutes:seconds:frames) timestamp to an LBA sector address.
fn msf_to_lba(minutes: u32, seconds: u32, frames: u32) -> u32 {
    minutes * SECONDS_PER_MINUTE * FRAMES_PER_SECOND + seconds * FRAMES_PER_SECOND + frames
}

/// Parses a "mm:ss:ff" timestamp string into (minutes, seconds, frames).
fn parse_msf(s: &str) -> Result<(u32, u32, u32), CdError> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 3 {
        return Err(CdError::ParseError(format!("invalid MSF timestamp: {s}")));
    }
    let minutes = parts[0]
        .parse::<u32>()
        .map_err(|_| CdError::ParseError(format!("invalid minutes in MSF: {s}")))?;
    let seconds = parts[1]
        .parse::<u32>()
        .map_err(|_| CdError::ParseError(format!("invalid seconds in MSF: {s}")))?;
    let frames = parts[2]
        .parse::<u32>()
        .map_err(|_| CdError::ParseError(format!("invalid frames in MSF: {s}")))?;
    Ok((minutes, seconds, frames))
}

/// Intermediate track data during CUE parsing.
struct CueTrack {
    number: u8,
    track_type: TrackType,
    sector_size: u16,
    sector_layout: SectorLayout,
    file_index: usize,
    index00_lba: Option<u32>,
    index01_lba: Option<u32>,
    pregap_sectors: u32,
}

struct CueSheet {
    file_names: Vec<String>,
    tracks: Vec<CueTrack>,
}

fn parse_cue_sheet(cue_content: &str) -> Result<CueSheet, CdError> {
    let mut tracks: Vec<CueTrack> = Vec::new();
    let mut file_names = Vec::new();
    let mut current_file_index = None;

    for line in cue_content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.is_empty() {
            continue;
        }

        match tokens[0].to_uppercase().as_str() {
            "FILE" => {
                if tokens.len() < 3 {
                    return Err(CdError::ParseError(
                        "FILE directive missing arguments".into(),
                    ));
                }
                let file_type = tokens.last().unwrap().to_uppercase();
                if file_type != "BINARY" {
                    return Err(CdError::UnsupportedFormat(format!(
                        "only BINARY file type is supported, got {file_type}"
                    )));
                }
                let rest = line[tokens[0].len()..].trim();
                let filename = if let Some(stripped) = rest.strip_prefix('"') {
                    stripped.split('"').next().unwrap_or("")
                } else {
                    tokens[1]
                };
                file_names.push(filename.to_string());
                current_file_index = Some(file_names.len() - 1);
            }
            "TRACK" => {
                let file_index = current_file_index
                    .ok_or_else(|| CdError::ParseError("TRACK before FILE directive".into()))?;
                if tokens.len() < 3 {
                    return Err(CdError::ParseError(
                        "TRACK directive missing arguments".into(),
                    ));
                }
                let number = tokens[1].parse::<u8>().map_err(|_| {
                    CdError::ParseError(format!("invalid track number: {}", tokens[1]))
                })?;
                let mode = tokens[2].to_uppercase();
                let (track_type, sector_size, sector_layout) = match mode.as_str() {
                    "MODE1/2352" => (TrackType::Data, SECTOR_SIZE_RAW, SectorLayout::RawMode1),
                    "MODE1/2048" => (TrackType::Data, SECTOR_SIZE_COOKED, SectorLayout::Cooked),
                    "MODE2/2352" => (TrackType::Data, SECTOR_SIZE_RAW, SectorLayout::RawMode2),
                    "MODE2/2048" => (TrackType::Data, SECTOR_SIZE_COOKED, SectorLayout::Cooked),
                    "AUDIO" => (TrackType::Audio, SECTOR_SIZE_RAW, SectorLayout::Audio),
                    _ => {
                        return Err(CdError::UnsupportedFormat(format!(
                            "unsupported track mode: {mode}"
                        )));
                    }
                };
                tracks.push(CueTrack {
                    number,
                    track_type,
                    sector_size,
                    sector_layout,
                    file_index,
                    index00_lba: None,
                    index01_lba: None,
                    pregap_sectors: 0,
                });
            }
            "INDEX" => {
                if tokens.len() < 3 {
                    return Err(CdError::ParseError(
                        "INDEX directive missing arguments".into(),
                    ));
                }
                let track = tracks
                    .last_mut()
                    .ok_or_else(|| CdError::ParseError("INDEX before TRACK".into()))?;
                let index_number = tokens[1].parse::<u8>().map_err(|_| {
                    CdError::ParseError(format!("invalid index number: {}", tokens[1]))
                })?;
                let (m, s, f) = parse_msf(tokens[2])?;
                let lba = msf_to_lba(m, s, f);
                match index_number {
                    0 => track.index00_lba = Some(lba),
                    1 => track.index01_lba = Some(lba),
                    _ => {}
                }
            }
            "PREGAP" => {
                if tokens.len() < 2 {
                    return Err(CdError::ParseError(
                        "PREGAP directive missing argument".into(),
                    ));
                }
                let track = tracks
                    .last_mut()
                    .ok_or_else(|| CdError::ParseError("PREGAP before TRACK".into()))?;
                let (m, s, f) = parse_msf(tokens[1])?;
                track.pregap_sectors = msf_to_lba(m, s, f);
            }
            "REM" | "CATALOG" | "PERFORMER" | "SONGWRITER" | "TITLE" | "ISRC" | "FLAGS" => {}
            _ => {}
        }
    }

    if tracks.is_empty() {
        return Err(CdError::ParseError("no tracks found in CUE sheet".into()));
    }
    for track in &tracks {
        if track.index01_lba.is_none() {
            return Err(CdError::ParseError(format!(
                "track {} missing INDEX 01",
                track.number
            )));
        }
    }

    Ok(CueSheet { file_names, tracks })
}

/// Extracts the BIN filename from a CUE sheet's FILE directive.
///
/// Parses the first `FILE "name" BINARY` line and returns the filename.
pub fn extract_bin_filename(cue_content: &str) -> Result<String, CdError> {
    extract_bin_filenames(cue_content)?
        .into_iter()
        .next()
        .ok_or_else(|| CdError::ParseError("no FILE directive found in CUE sheet".into()))
}

/// Extracts all BIN filenames from a CUE sheet's `FILE` directives in order.
pub fn extract_bin_filenames(cue_content: &str) -> Result<Vec<String>, CdError> {
    Ok(parse_cue_sheet(cue_content)?.file_names)
}

impl CdImage {
    /// Parses a CUE sheet and loads the associated BIN data into a `CdImage`.
    ///
    /// The `cue_content` is the text of the CUE file. The `bin_data` is the
    /// entire contents of the BIN file referenced by the CUE's FILE directive.
    pub fn from_cue(cue_content: &str, bin_data: Vec<u8>) -> Result<Self, CdError> {
        Self::from_cue_files(cue_content, vec![bin_data])
    }

    /// Parses a CUE sheet and loads the associated BIN files into a `CdImage`.
    pub fn from_cue_files(cue_content: &str, bin_files: Vec<Vec<u8>>) -> Result<Self, CdError> {
        let cue_sheet = parse_cue_sheet(cue_content)?;
        if cue_sheet.file_names.len() != bin_files.len() {
            return Err(CdError::ParseError(format!(
                "cue references {} files, but {} were provided",
                cue_sheet.file_names.len(),
                bin_files.len()
            )));
        }

        let mut file_offsets = Vec::with_capacity(bin_files.len());
        let total_bytes = bin_files.iter().map(Vec::len).sum();
        let mut data = Vec::with_capacity(total_bytes);
        for file_data in &bin_files {
            file_offsets.push(data.len() as u64);
            data.extend_from_slice(file_data);
        }

        let file_sector_counts: Vec<u32> = cue_sheet
            .tracks
            .iter()
            .map(|track| {
                let file_data = &bin_files[track.file_index];
                (file_data.len() / track.sector_size as usize) as u32
            })
            .collect();

        let mut tracks = Vec::with_capacity(cue_sheet.tracks.len());
        let mut next_disc_lba = 0u32;

        for (i, ct) in cue_sheet.tracks.iter().enumerate() {
            let index01_lba = ct.index01_lba.unwrap();
            let file_start_lba = ct.index00_lba.unwrap_or(index01_lba);
            let data_file_offset =
                file_offsets[ct.file_index] + u64::from(index01_lba) * u64::from(ct.sector_size);

            let sector_count = if i + 1 < cue_sheet.tracks.len()
                && cue_sheet.tracks[i + 1].file_index == ct.file_index
            {
                let next_start = cue_sheet.tracks[i + 1]
                    .index00_lba
                    .unwrap_or_else(|| cue_sheet.tracks[i + 1].index01_lba.unwrap());
                next_start.saturating_sub(index01_lba)
            } else {
                file_sector_counts[i].saturating_sub(index01_lba)
            };

            let pregap_lba = next_disc_lba;
            let start_lba = pregap_lba + index01_lba.saturating_sub(file_start_lba);

            tracks.push(Track {
                number: ct.number,
                track_type: ct.track_type,
                sector_size: ct.sector_size,
                start_lba,
                pregap_lba,
                sector_count,
                file_offset: data_file_offset,
                sector_layout: ct.sector_layout,
            });

            next_disc_lba = start_lba + sector_count;
        }

        Ok(CdImage {
            tracks,
            data,
            total_sectors: next_disc_lba,
        })
    }

    /// Returns the number of tracks.
    pub fn track_count(&self) -> u8 {
        self.tracks.len() as u8
    }

    /// Returns the total number of addressable sectors on the disc.
    pub fn total_sectors(&self) -> u32 {
        self.total_sectors
    }

    /// Returns a track by its 1-based track number.
    pub fn track(&self, number: u8) -> Option<&Track> {
        self.tracks.iter().find(|t| t.number == number)
    }

    /// Returns a slice of all tracks.
    pub fn tracks(&self) -> &[Track] {
        &self.tracks
    }

    /// Finds the track containing the given LBA.
    pub fn track_for_lba(&self, lba: u32) -> Option<&Track> {
        for track in self.tracks.iter().rev() {
            if lba >= track.start_lba {
                if lba < track.start_lba + track.sector_count {
                    return Some(track);
                }
                return None;
            }
        }
        None
    }

    /// Reads 2048 bytes of user data from the sector at the given LBA.
    ///
    /// For raw (2352-byte) sectors, extracts the 2048-byte user data portion
    /// starting at offset 16 (after sync + header). For cooked (2048-byte)
    /// sectors, returns the sector data directly.
    ///
    /// Returns the number of bytes copied, or `None` if the LBA is out of range.
    pub fn read_sector(&self, lba: u32, buf: &mut [u8]) -> Option<usize> {
        let track = self.track_for_lba(lba)?;
        let sector_offset = lba - track.start_lba;
        let byte_offset =
            track.file_offset + u64::from(sector_offset) * u64::from(track.sector_size);
        let byte_offset = byte_offset as usize;

        let copy_size = SECTOR_SIZE_COOKED as usize;
        if buf.len() < copy_size {
            return None;
        }

        let data_offset = match track.sector_layout {
            SectorLayout::Cooked => 0,
            SectorLayout::RawMode1 => RAW_MODE1_DATA_OFFSET,
            SectorLayout::RawMode2 => RAW_MODE2_DATA_OFFSET,
            SectorLayout::Audio => return None,
        };

        let start = byte_offset + data_offset;
        let end = start + copy_size;
        if end > self.data.len() {
            return None;
        }
        buf[..copy_size].copy_from_slice(&self.data[start..end]);

        Some(copy_size)
    }

    /// Reads a raw sector at the given LBA at the track's native sector size.
    ///
    /// Returns the number of bytes copied, or `None` if the LBA is out of range.
    pub fn read_sector_raw(&self, lba: u32, buf: &mut [u8]) -> Option<usize> {
        let track = self.track_for_lba(lba)?;
        let sector_offset = lba - track.start_lba;
        let byte_offset =
            track.file_offset + u64::from(sector_offset) * u64::from(track.sector_size);
        let byte_offset = byte_offset as usize;

        let copy_size = track.sector_size as usize;
        if buf.len() < copy_size {
            return None;
        }

        let end = byte_offset + copy_size;
        if end > self.data.len() {
            return None;
        }

        buf[..copy_size].copy_from_slice(&self.data[byte_offset..end]);
        Some(copy_size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn msf_to_lba_conversion() {
        assert_eq!(msf_to_lba(0, 0, 0), 0);
        assert_eq!(msf_to_lba(0, 0, 1), 1);
        assert_eq!(msf_to_lba(0, 1, 0), 75);
        assert_eq!(msf_to_lba(1, 0, 0), 4500);
        assert_eq!(msf_to_lba(0, 2, 0), 150);
        assert_eq!(msf_to_lba(1, 2, 3), 4653);
    }

    #[test]
    fn parse_msf_valid() {
        assert_eq!(parse_msf("00:00:00").unwrap(), (0, 0, 0));
        assert_eq!(parse_msf("01:02:03").unwrap(), (1, 2, 3));
        assert_eq!(parse_msf("72:30:00").unwrap(), (72, 30, 0));
    }

    #[test]
    fn parse_msf_invalid() {
        assert!(parse_msf("00:00").is_err());
        assert!(parse_msf("aa:bb:cc").is_err());
        assert!(parse_msf("").is_err());
    }

    fn make_raw_data_sector(user_data_byte: u8) -> Vec<u8> {
        let mut sector = vec![0u8; 2352];
        // Sync pattern (12 bytes).
        sector[0] = 0x00;
        for b in &mut sector[1..11] {
            *b = 0xFF;
        }
        sector[11] = 0x00;
        // Header (4 bytes): minute, second, frame, mode.
        sector[12] = 0x00;
        sector[13] = 0x02;
        sector[14] = 0x00;
        sector[15] = 0x01; // Mode 1
        // User data (2048 bytes).
        for b in &mut sector[16..16 + 2048] {
            *b = user_data_byte;
        }
        sector
    }

    #[test]
    fn single_data_track_cooked() {
        let cue = r#"FILE "test.bin" BINARY
  TRACK 01 MODE1/2048
    INDEX 01 00:00:00
"#;
        let sectors = 100u32;
        let bin_data = vec![0xABu8; sectors as usize * 2048];
        let image = CdImage::from_cue(cue, bin_data).unwrap();

        assert_eq!(image.track_count(), 1);
        assert_eq!(image.total_sectors(), 100);

        let track = image.track(1).unwrap();
        assert_eq!(track.number, 1);
        assert_eq!(track.track_type, TrackType::Data);
        assert_eq!(track.sector_size, 2048);
        assert_eq!(track.start_lba, 0);
        assert_eq!(track.sector_count, 100);

        let mut buf = [0u8; 2048];
        let n = image.read_sector(0, &mut buf).unwrap();
        assert_eq!(n, 2048);
        assert!(buf.iter().all(|&b| b == 0xAB));
    }

    #[test]
    fn single_data_track_raw() {
        let cue = r#"FILE "test.bin" BINARY
  TRACK 01 MODE1/2352
    INDEX 01 00:00:00
"#;
        let mut bin_data = Vec::new();
        for _ in 0..10 {
            bin_data.extend_from_slice(&make_raw_data_sector(0xCD));
        }
        let image = CdImage::from_cue(cue, bin_data).unwrap();

        assert_eq!(image.track_count(), 1);
        assert_eq!(image.total_sectors(), 10);

        // Read cooked (extracts user data from raw sector).
        let mut buf = [0u8; 2048];
        let n = image.read_sector(0, &mut buf).unwrap();
        assert_eq!(n, 2048);
        assert!(buf.iter().all(|&b| b == 0xCD));

        // Read raw (returns full 2352-byte sector).
        let mut raw_buf = [0u8; 2352];
        let n = image.read_sector_raw(0, &mut raw_buf).unwrap();
        assert_eq!(n, 2352);
        assert_eq!(raw_buf[15], 0x01); // Mode 1
    }

    #[test]
    fn multi_track_data_and_audio() {
        let cue = r#"FILE "game.bin" BINARY
  TRACK 01 MODE1/2352
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    INDEX 00 00:04:00
    INDEX 01 00:06:00
"#;
        // Total file: 300 sectors (track 1) + 150 pregap + 200 sectors (track 2 audio) = 650 sectors.
        let mut bin_data = Vec::new();
        // Track 1: 300 raw data sectors.
        for _ in 0..300 {
            bin_data.extend_from_slice(&make_raw_data_sector(0x11));
        }
        // Track 2 pregap (150 sectors of silence).
        for _ in 0..150 {
            bin_data.extend_from_slice(&[0u8; 2352]);
        }
        // Track 2: 200 audio sectors.
        for _ in 0..200 {
            bin_data.extend_from_slice(&[0xAAu8; 2352]);
        }

        let image = CdImage::from_cue(cue, bin_data).unwrap();

        assert_eq!(image.track_count(), 2);
        assert_eq!(image.total_sectors(), 650); // 450 + 200

        let t1 = image.track(1).unwrap();
        assert_eq!(t1.track_type, TrackType::Data);
        assert_eq!(t1.start_lba, 0);
        assert_eq!(t1.sector_count, 300);

        let t2 = image.track(2).unwrap();
        assert_eq!(t2.track_type, TrackType::Audio);
        assert_eq!(t2.start_lba, 450);
        assert_eq!(t2.pregap_lba, 300);
        assert_eq!(t2.sector_count, 200);

        // Read from track 1.
        let mut buf = [0u8; 2048];
        assert!(image.read_sector(0, &mut buf).is_some());
        assert!(buf.iter().all(|&b| b == 0x11));

        // Read from track 2 (audio, raw read).
        let mut raw_buf = [0u8; 2352];
        assert!(image.read_sector_raw(450, &mut raw_buf).is_some());
        assert!(raw_buf.iter().all(|&b| b == 0xAA));

        // Track lookup.
        assert_eq!(image.track_for_lba(0).unwrap().number, 1);
        assert_eq!(image.track_for_lba(299).unwrap().number, 1);
        assert_eq!(image.track_for_lba(450).unwrap().number, 2);
        assert_eq!(image.track_for_lba(649).unwrap().number, 2);

        // Out of range.
        assert!(image.track_for_lba(300).is_none()); // In pregap, not part of track 1 or track 2
        assert!(image.track_for_lba(650).is_none());
    }

    #[test]
    fn multi_file_cue_uses_file_relative_indices() {
        let cue = r#"FILE "track01.bin" BINARY
  TRACK 01 MODE1/2352
    INDEX 01 00:00:00
FILE "track02.bin" BINARY
  TRACK 02 AUDIO
    INDEX 00 00:00:00
    INDEX 01 00:02:00
"#;
        let file_one = vec![0x11u8; 4 * 2352];
        let file_two = vec![0xAAu8; 152 * 2352];

        let image = CdImage::from_cue_files(cue, vec![file_one, file_two]).unwrap();

        let track_one = image.track(1).unwrap();
        assert_eq!(track_one.start_lba, 0);
        assert_eq!(track_one.sector_count, 4);

        let track_two = image.track(2).unwrap();
        assert_eq!(track_two.pregap_lba, 4);
        assert_eq!(track_two.start_lba, 154);
        assert_eq!(track_two.sector_count, 2);

        let mut buf = [0u8; 2048];
        assert!(image.read_sector(0, &mut buf).is_some());

        let mut raw_buf = [0u8; 2352];
        assert!(image.read_sector_raw(154, &mut raw_buf).is_some());
        assert!(raw_buf.iter().all(|&byte| byte == 0xAA));
    }

    #[test]
    fn track_for_lba_empty_disc() {
        // Edge case: valid disc but LBA way out of range.
        let cue = r#"FILE "test.bin" BINARY
  TRACK 01 MODE1/2048
    INDEX 01 00:00:00
"#;
        let bin_data = vec![0u8; 2048 * 10];
        let image = CdImage::from_cue(cue, bin_data).unwrap();

        assert!(image.track_for_lba(10).is_none());
        assert!(image.track_for_lba(1000).is_none());
    }

    #[test]
    fn read_sector_buffer_too_small() {
        let cue = r#"FILE "test.bin" BINARY
  TRACK 01 MODE1/2048
    INDEX 01 00:00:00
"#;
        let bin_data = vec![0u8; 2048 * 5];
        let image = CdImage::from_cue(cue, bin_data).unwrap();

        let mut small_buf = [0u8; 1024];
        assert!(image.read_sector(0, &mut small_buf).is_none());
    }

    #[test]
    fn read_sector_out_of_range() {
        let cue = r#"FILE "test.bin" BINARY
  TRACK 01 MODE1/2048
    INDEX 01 00:00:00
"#;
        let bin_data = vec![0u8; 2048 * 5];
        let image = CdImage::from_cue(cue, bin_data).unwrap();

        let mut buf = [0u8; 2048];
        assert!(image.read_sector(5, &mut buf).is_none());
        assert!(image.read_sector(100, &mut buf).is_none());
    }

    #[test]
    fn cue_missing_file_directive() {
        let cue = r#"TRACK 01 MODE1/2048
    INDEX 01 00:00:00
"#;
        assert!(CdImage::from_cue(cue, vec![]).is_err());
    }

    #[test]
    fn cue_missing_index01() {
        let cue = r#"FILE "test.bin" BINARY
  TRACK 01 MODE1/2048
    INDEX 00 00:00:00
"#;
        assert!(CdImage::from_cue(cue, vec![]).is_err());
    }

    #[test]
    fn cue_no_tracks() {
        let cue = r#"FILE "test.bin" BINARY
"#;
        assert!(CdImage::from_cue(cue, vec![]).is_err());
    }

    #[test]
    fn cue_unsupported_file_type() {
        let cue = r#"FILE "test.wav" WAVE
  TRACK 01 AUDIO
    INDEX 01 00:00:00
"#;
        assert!(CdImage::from_cue(cue, vec![]).is_err());
    }

    #[test]
    fn cue_unsupported_track_mode() {
        let cue = r#"FILE "test.bin" BINARY
  TRACK 01 CDG
    INDEX 01 00:00:00
"#;
        assert!(CdImage::from_cue(cue, vec![]).is_err());
    }

    #[test]
    fn cue_with_pregap_directive() {
        let cue = r#"FILE "test.bin" BINARY
  TRACK 01 MODE1/2048
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    PREGAP 00:02:00
    INDEX 01 00:10:00
"#;
        // Track 1: LBA 0, sectors until track 2 INDEX 01 = 750.
        // Track 2: LBA 750, PREGAP is virtual (not in BIN file).
        // Next track's index00 is None, index01 is 750. So track 1 count = 750.
        let bin_data = vec![0u8; 2048 * 750 + 2352 * 100];
        let image = CdImage::from_cue(cue, bin_data).unwrap();

        assert_eq!(image.track_count(), 2);
        let t1 = image.track(1).unwrap();
        assert_eq!(t1.start_lba, 0);
        assert_eq!(t1.sector_count, 750);

        let t2 = image.track(2).unwrap();
        assert_eq!(t2.start_lba, 750);
        assert_eq!(t2.pregap_lba, 750); // No INDEX 00, so pregap_lba == start_lba.
    }

    #[test]
    fn cue_with_remarks_and_metadata() {
        let cue = r#"REM GENRE "Game"
REM DATE 1995
CATALOG 1234567890123
PERFORMER "Test Artist"
TITLE "Test Disc"
FILE "test.bin" BINARY
  TRACK 01 MODE1/2048
    ISRC JPXXX1234567
    FLAGS DCP
    INDEX 01 00:00:00
"#;
        let bin_data = vec![0u8; 2048 * 10];
        let image = CdImage::from_cue(cue, bin_data).unwrap();
        assert_eq!(image.track_count(), 1);
    }

    #[test]
    fn sector_read_at_track_boundary() {
        let cue = r#"FILE "test.bin" BINARY
  TRACK 01 MODE1/2048
    INDEX 01 00:00:00
  TRACK 02 MODE1/2048
    INDEX 01 00:02:00
"#;
        // Track 1: LBA 0-149 (150 sectors), Track 2: LBA 150+.
        let mut bin_data = vec![0x11u8; 2048 * 150]; // Track 1.
        bin_data.extend_from_slice(&vec![0x22u8; 2048 * 50]); // Track 2.

        let image = CdImage::from_cue(cue, bin_data).unwrap();

        let mut buf = [0u8; 2048];

        // Last sector of track 1.
        let n = image.read_sector(149, &mut buf).unwrap();
        assert_eq!(n, 2048);
        assert!(buf.iter().all(|&b| b == 0x11));

        // First sector of track 2.
        let n = image.read_sector(150, &mut buf).unwrap();
        assert_eq!(n, 2048);
        assert!(buf.iter().all(|&b| b == 0x22));
    }

    #[test]
    fn mode2_track() {
        let cue = r#"FILE "test.bin" BINARY
  TRACK 01 MODE2/2352
    INDEX 01 00:00:00
"#;
        let bin_data = vec![0u8; 2352 * 10];
        let image = CdImage::from_cue(cue, bin_data).unwrap();
        assert_eq!(image.track_count(), 1);
        assert_eq!(image.track(1).unwrap().track_type, TrackType::Data);
        assert_eq!(image.track(1).unwrap().sector_size, 2352);
    }

    #[test]
    fn mode2_track_cooked_read_uses_mode2_user_data_offset() {
        let cue = r#"FILE "test.bin" BINARY
  TRACK 01 MODE2/2352
    INDEX 01 00:00:00
"#;
        let mut sector = vec![0u8; 2352];
        sector[15] = 0x02;
        sector[16..24].copy_from_slice(&[0x11, 0x22, 0x33, 0x44, 0x11, 0x22, 0x33, 0x44]);
        sector[24] = 0x01;
        sector[25..30].copy_from_slice(b"CD001");
        sector[30] = 0x01;

        let image = CdImage::from_cue(cue, sector).unwrap();

        let mut buf = [0u8; 2048];
        let count = image.read_sector(0, &mut buf).unwrap();
        assert_eq!(count, 2048);
        assert_eq!(buf[0], 0x01);
        assert_eq!(&buf[1..6], b"CD001");
        assert_eq!(buf[6], 0x01);
    }
}

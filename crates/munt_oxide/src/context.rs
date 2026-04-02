// Copyright (C) 2003, 2004, 2005, 2006, 2008, 2009 Dean Beeler, Jerome Fisher
// Copyright (C) 2011-2026 Dean Beeler, Jerome Fisher, Sergey V. Mikayev
//
//  This program is free software: you can redistribute it and/or modify
//  it under the terms of the GNU Lesser General Public License as published by
//  the Free Software Foundation, either version 2.1 of the License, or
//  (at your option) any later version.
//
//  This program is distributed in the hope that it will be useful,
//  but WITHOUT ANY WARRANTY; without even the implied warranty of
//  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
//  GNU Lesser General Public License for more details.
//
//  You should have received a copy of the GNU Lesser General Public License
//  along with this program.  If not, see <http://www.gnu.org/licenses/>.

use std::{
    fmt, fs,
    path::{Path, PathBuf},
};

use common::info;

use crate::{
    rom_info::{self, PairType, RomInfo, RomType},
    state::{MuntState, SAMPLE_RATE},
};

pub(crate) struct MuntContext {
    state: MuntState,
    sample_rate: u32,
}

impl MuntContext {
    pub(crate) fn new(rom_directory: &Path) -> Result<Self, MuntContextError> {
        if !rom_directory.is_dir() {
            return Err(MuntContextError::DirectoryNotFound(
                rom_directory.to_path_buf(),
            ));
        }

        let entries = fs::read_dir(rom_directory).map_err(|error| {
            MuntContextError::DirectoryReadFailed(rom_directory.to_path_buf(), error)
        })?;

        let mut control_rom: Option<(Vec<u8>, &'static RomInfo)> = None;
        let mut pcm_rom: Option<(Vec<u8>, &'static RomInfo)> = None;

        // Collect all identified partial ROMs for potential pairing.
        let mut partial_control_roms: Vec<(Vec<u8>, &'static RomInfo)> = Vec::new();
        let mut partial_pcm_roms: Vec<(Vec<u8>, &'static RomInfo)> = Vec::new();

        for entry in entries.flatten() {
            let path = entry.path();
            let is_rom = path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("rom"));
            if !is_rom {
                continue;
            }

            let data = match fs::read(&path) {
                Ok(data) => data,
                Err(_) => continue,
            };

            let Some(info) = rom_info::get_rom_info(&data) else {
                continue;
            };

            match info.pair_type {
                PairType::Full => match info.rom_type {
                    RomType::Control => {
                        if control_rom.is_none() {
                            control_rom = Some((data, info));
                        }
                    }
                    RomType::Pcm => {
                        if pcm_rom.is_none() {
                            pcm_rom = Some((data, info));
                        }
                    }
                },
                _ => match info.rom_type {
                    RomType::Control => {
                        partial_control_roms.push((data, info));
                    }
                    RomType::Pcm => {
                        partial_pcm_roms.push((data, info));
                    }
                },
            }
        }

        // Try to assemble full ROMs from partial halves if we are missing a full one.
        if control_rom.is_none() {
            control_rom = try_merge_partials(&partial_control_roms);
        }
        if pcm_rom.is_none() {
            pcm_rom = try_merge_partials(&partial_pcm_roms);
        }

        let (control_data, control_info) =
            control_rom.ok_or(MuntContextError::NoRomsFound(rom_directory.to_path_buf()))?;
        let (pcm_data, pcm_info) =
            pcm_rom.ok_or(MuntContextError::NoRomsFound(rom_directory.to_path_buf()))?;

        info!(
            "MT-32 ROMs identified: {} + {}",
            control_info.description, pcm_info.description
        );

        let mut state = MuntState::default();

        if !crate::synth::open(
            &mut state,
            &control_data,
            &pcm_data,
            control_info.short_name,
        ) {
            return Err(MuntContextError::SynthOpenFailed(
                "synth::open returned false".to_string(),
            ));
        }

        let sample_rate = SAMPLE_RATE;

        info!("MT-32 synth opened successfully (munt_oxide, {sample_rate} Hz)");

        Ok(Self { state, sample_rate })
    }

    pub(crate) fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub(crate) fn parse_stream(&mut self, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        let state = &mut self.state;
        // We can't pass closures that capture &mut state to parse_stream since
        // parse_stream also needs &mut state.midi_stream_parser.
        // Instead, collect parsed events and process them after, preserving stream order.
        enum ParsedMidiMessage {
            Short(u32),
            Sysex(Vec<u8>),
        }
        let messages = core::cell::RefCell::new(Vec::new());
        state.midi_stream_parser.parse_stream(
            data,
            &mut |msg| {
                messages.borrow_mut().push(ParsedMidiMessage::Short(msg));
            },
            &mut |sysex| {
                messages
                    .borrow_mut()
                    .push(ParsedMidiMessage::Sysex(sysex.to_vec()));
            },
            &mut |_realtime| {},
        );
        for message in messages.into_inner() {
            match message {
                ParsedMidiMessage::Short(msg) => {
                    let _ = crate::synth::play_msg(state, msg);
                }
                ParsedMidiMessage::Sysex(sysex) => {
                    let _ = crate::synth::play_sysex(state, &sysex);
                }
            }
        }
    }

    pub(crate) fn render(&mut self, output: &mut [f32], num_frames: u32) {
        crate::synth::render(&mut self.state, output, num_frames);
    }
}

impl Drop for MuntContext {
    fn drop(&mut self) {
        crate::synth::close(&mut self.state);
    }
}

fn try_merge_partials(
    partials: &[(Vec<u8>, &'static RomInfo)],
) -> Option<(Vec<u8>, &'static RomInfo)> {
    for (i, (data_a, info_a)) in partials.iter().enumerate() {
        let Some(pair_index) = info_a.pair_rom_info_index else {
            continue;
        };
        for (data_b, info_b) in partials.iter().skip(i + 1) {
            let all = rom_info::get_all_rom_infos();
            if !std::ptr::eq(*info_b, &all[pair_index]) {
                continue;
            }
            let merged = merge_rom_pair(data_a, info_a, data_b, info_b);
            if let Some(merged_data) = merged {
                // After merging, identify the full ROM.
                if let Some(full_info) = rom_info::get_rom_info(&merged_data)
                    && full_info.pair_type == PairType::Full
                {
                    return Some((merged_data, full_info));
                }
            }
        }
    }
    None
}

fn merge_rom_pair(
    data_a: &[u8],
    info_a: &RomInfo,
    data_b: &[u8],
    info_b: &RomInfo,
) -> Option<Vec<u8>> {
    // Determine which is first/mux0 and which is second/mux1.
    let (first_data, second_data, first_info) = match (info_a.pair_type, info_b.pair_type) {
        (PairType::FirstHalf, PairType::SecondHalf) | (PairType::Mux0, PairType::Mux1) => {
            (data_a, data_b, info_a)
        }
        (PairType::SecondHalf, PairType::FirstHalf) | (PairType::Mux1, PairType::Mux0) => {
            (data_b, data_a, info_b)
        }
        _ => return None,
    };

    match first_info.pair_type {
        PairType::FirstHalf => {
            let mut merged = Vec::with_capacity(first_data.len() + second_data.len());
            merged.extend_from_slice(first_data);
            merged.extend_from_slice(second_data);
            Some(merged)
        }
        PairType::Mux0 => {
            let mut merged = vec![0u8; first_data.len() * 2];
            for (i, &byte) in first_data.iter().enumerate() {
                merged[i * 2] = byte;
            }
            for (i, &byte) in second_data.iter().enumerate() {
                merged[i * 2 + 1] = byte;
            }
            Some(merged)
        }
        _ => None,
    }
}

#[derive(Debug)]
pub enum MuntContextError {
    DirectoryNotFound(PathBuf),
    DirectoryReadFailed(PathBuf, std::io::Error),
    NoRomsFound(PathBuf),
    SynthOpenFailed(String),
}

impl fmt::Display for MuntContextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DirectoryNotFound(path) => {
                write!(f, "ROM directory not found: {}", path.display())
            }
            Self::DirectoryReadFailed(path, error) => {
                write!(
                    f,
                    "failed to read ROM directory {}: {error}",
                    path.display()
                )
            }
            Self::NoRomsFound(path) => {
                write!(f, "no MT-32 ROM files found in {}", path.display())
            }
            Self::SynthOpenFailed(reason) => {
                write!(f, "failed to open MT-32 synth: {reason}")
            }
        }
    }
}

impl std::error::Error for MuntContextError {}

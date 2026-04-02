// Copyright (C) 2003, 2004, 2005, 2006, 2008, 2009 Dean Beeler, Jerome Fisher
// Copyright (C) 2011-2024 Dean Beeler, Jerome Fisher, Sergey V. Mikayev
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

use crate::sha1::{sha1_calc, sha1_from_hex};

// Defines vital info about ROM file to be used by synth and applications.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RomType {
    Pcm,
    Control,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PairType {
    /// Complete ROM image ready to use with Synth.
    Full,
    /// ROM image contains data that occupies lower addresses. Needs pairing before use.
    FirstHalf,
    /// ROM image contains data that occupies higher addresses. Needs pairing before use.
    SecondHalf,
    /// ROM image contains data that occupies even addresses. Needs pairing before use.
    Mux0,
    /// ROM image contains data that occupies odd addresses. Needs pairing before use.
    Mux1,
}

#[derive(Debug, Clone)]
pub(crate) struct RomInfo {
    pub(crate) file_size: usize,
    pub(crate) sha1_digest: [u8; 20],
    pub(crate) rom_type: RomType,
    pub(crate) short_name: &'static str,
    pub(crate) description: &'static str,
    pub(crate) pair_type: PairType,
    /// Index into ALL_ROM_INFOS for the corresponding other image for pairing, or None for Full images.
    pub(crate) pair_rom_info_index: Option<usize>,
}

/// Indices into ALL_ROM_INFOS. Full ROMs come first, then partial ROMs.
pub(crate) const FULL_ROM_COUNT: usize = 14;

// Partial ROMs (indices 14..27):
const IDX_CTRL_MT32_V1_04_A: usize = 14;
const IDX_CTRL_MT32_V1_04_B: usize = 15;
const IDX_CTRL_MT32_V1_05_A: usize = 16;
const IDX_CTRL_MT32_V1_05_B: usize = 17;
const IDX_CTRL_MT32_V1_06_A: usize = 18;
const IDX_CTRL_MT32_V1_06_B: usize = 19;
const IDX_CTRL_MT32_V1_07_A: usize = 20;
const IDX_CTRL_MT32_V1_07_B: usize = 21;
const IDX_CTRL_MT32_BLUER_A: usize = 22;
const IDX_CTRL_MT32_BLUER_B: usize = 23;
const IDX_PCM_MT32_L: usize = 24;
const IDX_PCM_MT32_H: usize = 25;
// Alias of PCM_MT32 ROM, only useful for pairing with PCM_CM32L_H.
const IDX_PCM_CM32L_L: usize = 26;
const IDX_PCM_CM32L_H: usize = 27;

pub(crate) const PARTIAL_ROM_COUNT: usize = 14;
pub(crate) const ALL_ROM_COUNT: usize = FULL_ROM_COUNT + PARTIAL_ROM_COUNT;

pub(crate) fn get_all_rom_infos() -> &'static [RomInfo; ALL_ROM_COUNT] {
    use std::sync::LazyLock;
    static ALL_ROM_INFOS: LazyLock<[RomInfo; ALL_ROM_COUNT]> = LazyLock::new(build_rom_info_table);
    &ALL_ROM_INFOS
}

fn build_rom_info_table() -> [RomInfo; ALL_ROM_COUNT] {
    // SHA-1 digests for control ROMs.
    let ctrl_mt32_v1_04_a_sha1 = sha1_from_hex("9cd4858014c4e8a9dff96053f784bfaac1092a2e");
    let ctrl_mt32_v1_04_b_sha1 = sha1_from_hex("fe8db469b5bfeb37edb269fd47e3ce6d91014652");
    let ctrl_mt32_v1_04_sha1 = sha1_from_hex("5a5cb5a77d7d55ee69657c2f870416daed52dea7");
    let ctrl_mt32_v1_05_a_sha1 = sha1_from_hex("57a09d80d2f7ca5b9734edbe9645e6e700f83701");
    let ctrl_mt32_v1_05_b_sha1 = sha1_from_hex("52e3c6666db9ef962591a8ee99be0cde17f3a6b6");
    let ctrl_mt32_v1_05_sha1 = sha1_from_hex("e17a3a6d265bf1fa150312061134293d2b58288c");
    let ctrl_mt32_v1_06_a_sha1 = sha1_from_hex("cc83bf23cee533097fb4c7e2c116e43b50ebacc8");
    let ctrl_mt32_v1_06_b_sha1 = sha1_from_hex("bf4f15666bc46679579498386704893b630c1171");
    let ctrl_mt32_v1_06_sha1 = sha1_from_hex("a553481f4e2794c10cfe597fef154eef0d8257de");
    let ctrl_mt32_v1_07_a_sha1 = sha1_from_hex("13f06b38f0d9e0fc050b6503ab777bb938603260");
    let ctrl_mt32_v1_07_b_sha1 = sha1_from_hex("c55e165487d71fa88bd8c5e9c083bc456c1a89aa");
    let ctrl_mt32_v1_07_sha1 = sha1_from_hex("b083518fffb7f66b03c23b7eb4f868e62dc5a987");
    let ctrl_mt32_bluer_a_sha1 = sha1_from_hex("11a6ae5d8b6ee328b371af7f1e40b82125aa6b4d");
    let ctrl_mt32_bluer_b_sha1 = sha1_from_hex("e0934320d7cbb5edfaa29e0d01ae835ef620085b");
    let ctrl_mt32_bluer_sha1 = sha1_from_hex("7b8c2a5ddb42fd0732e2f22b3340dcf5360edf92");

    let ctrl_mt32_v2_03_sha1 = sha1_from_hex("5837064c9df4741a55f7c4d8787ac158dff2d3ce");
    let ctrl_mt32_v2_04_sha1 = sha1_from_hex("2c16432b6c73dd2a3947cba950a0f4c19d6180eb");
    let ctrl_mt32_v2_06_sha1 = sha1_from_hex("2869cf4c235d671668cfcb62415e2ce8323ad4ed");
    let ctrl_mt32_v2_07_sha1 = sha1_from_hex("47b52adefedaec475c925e54340e37673c11707c");
    let ctrl_cm32l_v1_00_sha1 = sha1_from_hex("73683d585cd6948cc19547942ca0e14a0319456d");
    let ctrl_cm32l_v1_02_sha1 = sha1_from_hex("a439fbb390da38cada95a7cbb1d6ca199cd66ef8");
    let ctrl_cm32ln_v1_00_sha1 = sha1_from_hex("dc1c5b1b90a4646d00f7daf3679733c7badc7077");

    // SHA-1 digests for PCM ROMs.
    let pcm_mt32_l_sha1 = sha1_from_hex("3a1e19b0cd4036623fd1d1d11f5f25995585962b");
    let pcm_mt32_h_sha1 = sha1_from_hex("2cadb99d21a6a4a6f5b61b6218d16e9b43f61d01");
    let pcm_mt32_sha1 = sha1_from_hex("f6b1eebc4b2d200ec6d3d21d51325d5b48c60252");
    let pcm_cm32l_h_sha1 = sha1_from_hex("3ad889fde5db5b6437cbc2eb6e305312fec3df93");
    let pcm_cm32l_sha1 = sha1_from_hex("289cc298ad532b702461bfc738009d9ebe8025ea");

    [
        // Full ROMs (indices 0..14).
        RomInfo {
            file_size: 65536,
            sha1_digest: ctrl_mt32_v1_04_sha1,
            rom_type: RomType::Control,
            short_name: "ctrl_mt32_1_04",
            description: "MT-32 Control v1.04",
            pair_type: PairType::Full,
            pair_rom_info_index: None,
        },
        RomInfo {
            file_size: 65536,
            sha1_digest: ctrl_mt32_v1_05_sha1,
            rom_type: RomType::Control,
            short_name: "ctrl_mt32_1_05",
            description: "MT-32 Control v1.05",
            pair_type: PairType::Full,
            pair_rom_info_index: None,
        },
        RomInfo {
            file_size: 65536,
            sha1_digest: ctrl_mt32_v1_06_sha1,
            rom_type: RomType::Control,
            short_name: "ctrl_mt32_1_06",
            description: "MT-32 Control v1.06",
            pair_type: PairType::Full,
            pair_rom_info_index: None,
        },
        RomInfo {
            file_size: 65536,
            sha1_digest: ctrl_mt32_v1_07_sha1,
            rom_type: RomType::Control,
            short_name: "ctrl_mt32_1_07",
            description: "MT-32 Control v1.07",
            pair_type: PairType::Full,
            pair_rom_info_index: None,
        },
        RomInfo {
            file_size: 65536,
            sha1_digest: ctrl_mt32_bluer_sha1,
            rom_type: RomType::Control,
            short_name: "ctrl_mt32_bluer",
            description: "MT-32 Control BlueRidge",
            pair_type: PairType::Full,
            pair_rom_info_index: None,
        },
        RomInfo {
            file_size: 131072,
            sha1_digest: ctrl_mt32_v2_03_sha1,
            rom_type: RomType::Control,
            short_name: "ctrl_mt32_2_03",
            description: "MT-32 Control v2.03",
            pair_type: PairType::Full,
            pair_rom_info_index: None,
        },
        RomInfo {
            file_size: 131072,
            sha1_digest: ctrl_mt32_v2_04_sha1,
            rom_type: RomType::Control,
            short_name: "ctrl_mt32_2_04",
            description: "MT-32 Control v2.04",
            pair_type: PairType::Full,
            pair_rom_info_index: None,
        },
        RomInfo {
            file_size: 131072,
            sha1_digest: ctrl_mt32_v2_06_sha1,
            rom_type: RomType::Control,
            short_name: "ctrl_mt32_2_06",
            description: "MT-32 Control v2.06",
            pair_type: PairType::Full,
            pair_rom_info_index: None,
        },
        RomInfo {
            file_size: 131072,
            sha1_digest: ctrl_mt32_v2_07_sha1,
            rom_type: RomType::Control,
            short_name: "ctrl_mt32_2_07",
            description: "MT-32 Control v2.07",
            pair_type: PairType::Full,
            pair_rom_info_index: None,
        },
        RomInfo {
            file_size: 65536,
            sha1_digest: ctrl_cm32l_v1_00_sha1,
            rom_type: RomType::Control,
            short_name: "ctrl_cm32l_1_00",
            description: "CM-32L/LAPC-I Control v1.00",
            pair_type: PairType::Full,
            pair_rom_info_index: None,
        },
        RomInfo {
            file_size: 65536,
            sha1_digest: ctrl_cm32l_v1_02_sha1,
            rom_type: RomType::Control,
            short_name: "ctrl_cm32l_1_02",
            description: "CM-32L/LAPC-I Control v1.02",
            pair_type: PairType::Full,
            pair_rom_info_index: None,
        },
        RomInfo {
            file_size: 65536,
            sha1_digest: ctrl_cm32ln_v1_00_sha1,
            rom_type: RomType::Control,
            short_name: "ctrl_cm32ln_1_00",
            description: "CM-32LN/CM-500/LAPC-N Control v1.00",
            pair_type: PairType::Full,
            pair_rom_info_index: None,
        },
        RomInfo {
            file_size: 524288,
            sha1_digest: pcm_mt32_sha1,
            rom_type: RomType::Pcm,
            short_name: "pcm_mt32",
            description: "MT-32 PCM ROM",
            pair_type: PairType::Full,
            pair_rom_info_index: None,
        },
        RomInfo {
            file_size: 1048576,
            sha1_digest: pcm_cm32l_sha1,
            rom_type: RomType::Pcm,
            short_name: "pcm_cm32l",
            description: "CM-32L/CM-64/LAPC-I PCM ROM",
            pair_type: PairType::Full,
            pair_rom_info_index: None,
        },
        // Partial ROMs (indices 14..28).
        RomInfo {
            file_size: 32768,
            sha1_digest: ctrl_mt32_v1_04_a_sha1,
            rom_type: RomType::Control,
            short_name: "ctrl_mt32_1_04_a",
            description: "MT-32 Control v1.04",
            pair_type: PairType::Mux0,
            pair_rom_info_index: Some(IDX_CTRL_MT32_V1_04_B),
        },
        RomInfo {
            file_size: 32768,
            sha1_digest: ctrl_mt32_v1_04_b_sha1,
            rom_type: RomType::Control,
            short_name: "ctrl_mt32_1_04_b",
            description: "MT-32 Control v1.04",
            pair_type: PairType::Mux1,
            pair_rom_info_index: Some(IDX_CTRL_MT32_V1_04_A),
        },
        RomInfo {
            file_size: 32768,
            sha1_digest: ctrl_mt32_v1_05_a_sha1,
            rom_type: RomType::Control,
            short_name: "ctrl_mt32_1_05_a",
            description: "MT-32 Control v1.05",
            pair_type: PairType::Mux0,
            pair_rom_info_index: Some(IDX_CTRL_MT32_V1_05_B),
        },
        RomInfo {
            file_size: 32768,
            sha1_digest: ctrl_mt32_v1_05_b_sha1,
            rom_type: RomType::Control,
            short_name: "ctrl_mt32_1_05_b",
            description: "MT-32 Control v1.05",
            pair_type: PairType::Mux1,
            pair_rom_info_index: Some(IDX_CTRL_MT32_V1_05_A),
        },
        RomInfo {
            file_size: 32768,
            sha1_digest: ctrl_mt32_v1_06_a_sha1,
            rom_type: RomType::Control,
            short_name: "ctrl_mt32_1_06_a",
            description: "MT-32 Control v1.06",
            pair_type: PairType::Mux0,
            pair_rom_info_index: Some(IDX_CTRL_MT32_V1_06_B),
        },
        RomInfo {
            file_size: 32768,
            sha1_digest: ctrl_mt32_v1_06_b_sha1,
            rom_type: RomType::Control,
            short_name: "ctrl_mt32_1_06_b",
            description: "MT-32 Control v1.06",
            pair_type: PairType::Mux1,
            pair_rom_info_index: Some(IDX_CTRL_MT32_V1_06_A),
        },
        RomInfo {
            file_size: 32768,
            sha1_digest: ctrl_mt32_v1_07_a_sha1,
            rom_type: RomType::Control,
            short_name: "ctrl_mt32_1_07_a",
            description: "MT-32 Control v1.07",
            pair_type: PairType::Mux0,
            pair_rom_info_index: Some(IDX_CTRL_MT32_V1_07_B),
        },
        RomInfo {
            file_size: 32768,
            sha1_digest: ctrl_mt32_v1_07_b_sha1,
            rom_type: RomType::Control,
            short_name: "ctrl_mt32_1_07_b",
            description: "MT-32 Control v1.07",
            pair_type: PairType::Mux1,
            pair_rom_info_index: Some(IDX_CTRL_MT32_V1_07_A),
        },
        RomInfo {
            file_size: 32768,
            sha1_digest: ctrl_mt32_bluer_a_sha1,
            rom_type: RomType::Control,
            short_name: "ctrl_mt32_bluer_a",
            description: "MT-32 Control BlueRidge",
            pair_type: PairType::Mux0,
            pair_rom_info_index: Some(IDX_CTRL_MT32_BLUER_B),
        },
        RomInfo {
            file_size: 32768,
            sha1_digest: ctrl_mt32_bluer_b_sha1,
            rom_type: RomType::Control,
            short_name: "ctrl_mt32_bluer_b",
            description: "MT-32 Control BlueRidge",
            pair_type: PairType::Mux1,
            pair_rom_info_index: Some(IDX_CTRL_MT32_BLUER_A),
        },
        RomInfo {
            file_size: 262144,
            sha1_digest: pcm_mt32_l_sha1,
            rom_type: RomType::Pcm,
            short_name: "pcm_mt32_l",
            description: "MT-32 PCM ROM",
            pair_type: PairType::FirstHalf,
            pair_rom_info_index: Some(IDX_PCM_MT32_H),
        },
        RomInfo {
            file_size: 262144,
            sha1_digest: pcm_mt32_h_sha1,
            rom_type: RomType::Pcm,
            short_name: "pcm_mt32_h",
            description: "MT-32 PCM ROM",
            pair_type: PairType::SecondHalf,
            pair_rom_info_index: Some(IDX_PCM_MT32_L),
        },
        // Alias of PCM_MT32 ROM, only useful for pairing with PCM_CM32L_H.
        RomInfo {
            file_size: 524288,
            sha1_digest: pcm_mt32_sha1,
            rom_type: RomType::Pcm,
            short_name: "pcm_cm32l_l",
            description: "CM-32L/CM-64/LAPC-I PCM ROM",
            pair_type: PairType::FirstHalf,
            pair_rom_info_index: Some(IDX_PCM_CM32L_H),
        },
        RomInfo {
            file_size: 524288,
            sha1_digest: pcm_cm32l_h_sha1,
            rom_type: RomType::Pcm,
            short_name: "pcm_cm32l_h",
            description: "CM-32L/CM-64/LAPC-I PCM ROM",
            pair_type: PairType::SecondHalf,
            pair_rom_info_index: Some(IDX_PCM_CM32L_L),
        },
    ]
}

/// Returns a RomInfo by inspecting the size and the SHA1 hash of the data
/// among all known RomInfos.
pub(crate) fn get_rom_info(data: &[u8]) -> Option<&'static RomInfo> {
    get_rom_info_from_list(data, get_all_rom_infos())
}

/// Returns a RomInfo by inspecting the size and the SHA1 hash of the data
/// among the RomInfos in the provided list.
pub(crate) fn get_rom_info_from_list<'a>(
    data: &[u8],
    rom_infos: &'a [RomInfo],
) -> Option<&'a RomInfo> {
    let file_size = data.len();
    let sha1 = sha1_calc(data);
    rom_infos
        .iter()
        .find(|rom_info| file_size == rom_info.file_size && sha1 == rom_info.sha1_digest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rom_info_table_count() {
        let all = get_all_rom_infos();
        assert_eq!(all.len(), ALL_ROM_COUNT);
    }

    #[test]
    fn test_rom_info_pair_references() {
        let all = get_all_rom_infos();
        for (i, info) in all.iter().enumerate() {
            if let Some(pair_idx) = info.pair_rom_info_index {
                assert!(
                    pair_idx < ALL_ROM_COUNT,
                    "ROM {} has out-of-bounds pair index",
                    i
                );
                let pair = &all[pair_idx];
                // The pair should also point back to us.
                assert_eq!(
                    pair.pair_rom_info_index,
                    Some(i),
                    "ROM {} ({}) pair ROM {} ({}) does not point back",
                    i,
                    info.short_name,
                    pair_idx,
                    pair.short_name,
                );
            }
        }
    }
}

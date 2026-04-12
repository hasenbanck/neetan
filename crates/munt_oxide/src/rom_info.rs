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

use crate::blake3_digest::{blake3_digest, blake3_digest_from_hex};

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
    pub(crate) blake3_digest: [u8; 32],
    pub(crate) rom_type: RomType,
    pub(crate) short_name: &'static str,
    pub(crate) description: &'static str,
    pub(crate) pair_type: PairType,
    pub(crate) pair_rom_info_index: Option<usize>,
}

/// Indices into ALL_ROM_INFOS. Full ROMs come first, then partial ROMs.
pub(crate) const FULL_ROM_COUNT: usize = 13;

const IDX_CTRL_MT32_V1_04_A: usize = FULL_ROM_COUNT;
const IDX_CTRL_MT32_V1_04_B: usize = IDX_CTRL_MT32_V1_04_A + 1;
const IDX_CTRL_MT32_V1_05_A: usize = IDX_CTRL_MT32_V1_04_B + 1;
const IDX_CTRL_MT32_V1_05_B: usize = IDX_CTRL_MT32_V1_05_A + 1;
const IDX_CTRL_MT32_V1_06_A: usize = IDX_CTRL_MT32_V1_05_B + 1;
const IDX_CTRL_MT32_V1_06_B: usize = IDX_CTRL_MT32_V1_06_A + 1;
const IDX_CTRL_MT32_V1_07_A: usize = IDX_CTRL_MT32_V1_06_B + 1;
const IDX_CTRL_MT32_V1_07_B: usize = IDX_CTRL_MT32_V1_07_A + 1;
const IDX_CTRL_MT32_BLUER_A: usize = IDX_CTRL_MT32_V1_07_B + 1;
const IDX_CTRL_MT32_BLUER_B: usize = IDX_CTRL_MT32_BLUER_A + 1;
const IDX_PCM_MT32_L: usize = IDX_CTRL_MT32_BLUER_B + 1;
const IDX_PCM_MT32_H: usize = IDX_PCM_MT32_L + 1;
const IDX_PCM_CM32L_L: usize = IDX_PCM_MT32_H + 1;
const IDX_PCM_CM32L_H: usize = IDX_PCM_CM32L_L + 1;

pub(crate) const PARTIAL_ROM_COUNT: usize = 14;
pub(crate) const ALL_ROM_COUNT: usize = FULL_ROM_COUNT + PARTIAL_ROM_COUNT;

pub(crate) fn get_all_rom_infos() -> &'static [RomInfo; ALL_ROM_COUNT] {
    use std::sync::LazyLock;
    static ALL_ROM_INFOS: LazyLock<[RomInfo; ALL_ROM_COUNT]> = LazyLock::new(build_rom_info_table);
    &ALL_ROM_INFOS
}

fn build_rom_info_table() -> [RomInfo; ALL_ROM_COUNT] {
    let ctrl_mt32_v1_04_a_blake3 =
        blake3_digest_from_hex("3b0bdc08828f383711334a5db13252b98df79cbd9fa7a21cd37e55355dd41963");
    let ctrl_mt32_v1_04_b_blake3 =
        blake3_digest_from_hex("a3feacf1522d04d283fcb20c262f8cdfe469a667eb4e4689899b168660923993");
    let ctrl_mt32_v1_04_blake3 =
        blake3_digest_from_hex("9102699229706ff459a718924884559d50a6a8749a2d27fe58548f3c0606f66a");
    let ctrl_mt32_v1_05_a_blake3 =
        blake3_digest_from_hex("2d970225f29d20dc38ef47e48db1ded49ee223f27a6b8c0e9072b55ebe85aa0f");
    let ctrl_mt32_v1_05_b_blake3 =
        blake3_digest_from_hex("a6d5c9d616cf23b8fdf06f86a8c1a3116b4bf71985ca337cca21ba517614dd04");
    let ctrl_mt32_v1_05_blake3 =
        blake3_digest_from_hex("6b05c40c21d67c6780c39dac669dc7869d2b9fbde62bfc73a03ec3634282658f");
    let ctrl_mt32_v1_06_a_blake3 =
        blake3_digest_from_hex("ad9dd4a7eec18b561ca9bfdf446730ff55019dc8e9a20b0e6de3a9c721282e68");
    let ctrl_mt32_v1_06_b_blake3 =
        blake3_digest_from_hex("7bd393b0b2dec1ee98b06357eb3849aa903bdd57595b0d1b409530e2a963269a");
    let ctrl_mt32_v1_06_blake3 =
        blake3_digest_from_hex("93e8a9bd5fdea0f3e92d9a9949e307bc98dc7d9ff7650b28d9dbfd2e863054bb");
    let ctrl_mt32_v1_07_a_blake3 =
        blake3_digest_from_hex("d8f51c813aebfa8f47a20ec8d5dc1bd870720b19d2ae43a20eea19612fb249d1");
    let ctrl_mt32_v1_07_b_blake3 =
        blake3_digest_from_hex("31f0fa94dda0bb836106b53c21d78e016bf970ea794a6cd84dbf4bd852ada51e");
    let ctrl_mt32_v1_07_blake3 =
        blake3_digest_from_hex("8f123c1f38104a2a7eb1df35fd5b26ca1b857185086a87233b355510264602bf");
    let ctrl_mt32_bluer_a_blake3 =
        blake3_digest_from_hex("848350fb882dbffafaa18fa4c100c2c63fec6ddc99ac62243dcf7acf86594397");
    let ctrl_mt32_bluer_b_blake3 =
        blake3_digest_from_hex("46a2c0b8ee01ed06a73bb3cfaee40199e8fcb51162e1504c32bb33dc32935dbb");
    let ctrl_mt32_bluer_blake3 =
        blake3_digest_from_hex("af3cc9fe2f9844adde07377af66b4e1b0636df499abf4f2cdba716bb886642ad");

    let ctrl_mt32_v2_04_blake3 =
        blake3_digest_from_hex("788364d4f8dbe7577f092ef944418461b65bdbd449e2808a3403e28e90c4ee5d");
    let ctrl_mt32_v2_06_blake3 =
        blake3_digest_from_hex("3bd5adf2aba6f5bd9a85d52dc164b2c0efd3c8e69b7cf058d4dcc644c85d98b3");
    let ctrl_mt32_v2_07_blake3 =
        blake3_digest_from_hex("eb32a5640adba7da5e5cc2b8a455cf709d9f8998f3a5b5f2f2aa948c0ff3a9e0");
    let ctrl_cm32l_v1_00_blake3 =
        blake3_digest_from_hex("d88dcc0e94864040bd5933d89a29afd5a156eb43fec416ae1add5c02e565b9ff");
    let ctrl_cm32l_v1_02_blake3 =
        blake3_digest_from_hex("136741df33c185e809b057ee82b71ad94a07e82925fb0b7941bdd5912be6f549");
    let ctrl_cm32ln_v1_00_blake3 =
        blake3_digest_from_hex("0037be2e04ee72b01de1577b996887cd4258ddb538a433b52de8f60829e06ce1");

    let pcm_mt32_l_blake3 =
        blake3_digest_from_hex("5ced158f0131b5170219cd69d438288321810004f06148eda275c11d3c488bfb");
    let pcm_mt32_h_blake3 =
        blake3_digest_from_hex("22a2f889408003c128a28a9672c11655444b2c777955114ba87d0fba5822d035");
    let pcm_mt32_blake3 =
        blake3_digest_from_hex("7805996b758fab5469e96d9a28588eb2e991440242372f7546345cdc66c8d97a");
    let pcm_cm32l_h_blake3 =
        blake3_digest_from_hex("991388440296b3ae2664f9f620667b64120ba862a1cda23e0701859693830397");
    let pcm_cm32l_blake3 =
        blake3_digest_from_hex("5e4839e75ec9e9b03eca0c0eacf4d4b551e76504c72c10325a311bd9ea1309e7");

    [
        RomInfo {
            file_size: 65536,
            blake3_digest: ctrl_mt32_v1_04_blake3,
            rom_type: RomType::Control,
            short_name: "ctrl_mt32_1_04",
            description: "MT-32 Control v1.04",
            pair_type: PairType::Full,
            pair_rom_info_index: None,
        },
        RomInfo {
            file_size: 65536,
            blake3_digest: ctrl_mt32_v1_05_blake3,
            rom_type: RomType::Control,
            short_name: "ctrl_mt32_1_05",
            description: "MT-32 Control v1.05",
            pair_type: PairType::Full,
            pair_rom_info_index: None,
        },
        RomInfo {
            file_size: 65536,
            blake3_digest: ctrl_mt32_v1_06_blake3,
            rom_type: RomType::Control,
            short_name: "ctrl_mt32_1_06",
            description: "MT-32 Control v1.06",
            pair_type: PairType::Full,
            pair_rom_info_index: None,
        },
        RomInfo {
            file_size: 65536,
            blake3_digest: ctrl_mt32_v1_07_blake3,
            rom_type: RomType::Control,
            short_name: "ctrl_mt32_1_07",
            description: "MT-32 Control v1.07",
            pair_type: PairType::Full,
            pair_rom_info_index: None,
        },
        RomInfo {
            file_size: 65536,
            blake3_digest: ctrl_mt32_bluer_blake3,
            rom_type: RomType::Control,
            short_name: "ctrl_mt32_bluer",
            description: "MT-32 Control BlueRidge",
            pair_type: PairType::Full,
            pair_rom_info_index: None,
        },
        RomInfo {
            file_size: 131072,
            blake3_digest: ctrl_mt32_v2_04_blake3,
            rom_type: RomType::Control,
            short_name: "ctrl_mt32_2_04",
            description: "MT-32 Control v2.04",
            pair_type: PairType::Full,
            pair_rom_info_index: None,
        },
        RomInfo {
            file_size: 131072,
            blake3_digest: ctrl_mt32_v2_06_blake3,
            rom_type: RomType::Control,
            short_name: "ctrl_mt32_2_06",
            description: "MT-32 Control v2.06",
            pair_type: PairType::Full,
            pair_rom_info_index: None,
        },
        RomInfo {
            file_size: 131072,
            blake3_digest: ctrl_mt32_v2_07_blake3,
            rom_type: RomType::Control,
            short_name: "ctrl_mt32_2_07",
            description: "MT-32 Control v2.07",
            pair_type: PairType::Full,
            pair_rom_info_index: None,
        },
        RomInfo {
            file_size: 65536,
            blake3_digest: ctrl_cm32l_v1_00_blake3,
            rom_type: RomType::Control,
            short_name: "ctrl_cm32l_1_00",
            description: "CM-32L/LAPC-I Control v1.00",
            pair_type: PairType::Full,
            pair_rom_info_index: None,
        },
        RomInfo {
            file_size: 65536,
            blake3_digest: ctrl_cm32l_v1_02_blake3,
            rom_type: RomType::Control,
            short_name: "ctrl_cm32l_1_02",
            description: "CM-32L/LAPC-I Control v1.02",
            pair_type: PairType::Full,
            pair_rom_info_index: None,
        },
        RomInfo {
            file_size: 65536,
            blake3_digest: ctrl_cm32ln_v1_00_blake3,
            rom_type: RomType::Control,
            short_name: "ctrl_cm32ln_1_00",
            description: "CM-32LN/CM-500/LAPC-N Control v1.00",
            pair_type: PairType::Full,
            pair_rom_info_index: None,
        },
        RomInfo {
            file_size: 524288,
            blake3_digest: pcm_mt32_blake3,
            rom_type: RomType::Pcm,
            short_name: "pcm_mt32",
            description: "MT-32 PCM ROM",
            pair_type: PairType::Full,
            pair_rom_info_index: None,
        },
        RomInfo {
            file_size: 1048576,
            blake3_digest: pcm_cm32l_blake3,
            rom_type: RomType::Pcm,
            short_name: "pcm_cm32l",
            description: "CM-32L/CM-64/LAPC-I PCM ROM",
            pair_type: PairType::Full,
            pair_rom_info_index: None,
        },
        RomInfo {
            file_size: 32768,
            blake3_digest: ctrl_mt32_v1_04_a_blake3,
            rom_type: RomType::Control,
            short_name: "ctrl_mt32_1_04_a",
            description: "MT-32 Control v1.04",
            pair_type: PairType::Mux0,
            pair_rom_info_index: Some(IDX_CTRL_MT32_V1_04_B),
        },
        RomInfo {
            file_size: 32768,
            blake3_digest: ctrl_mt32_v1_04_b_blake3,
            rom_type: RomType::Control,
            short_name: "ctrl_mt32_1_04_b",
            description: "MT-32 Control v1.04",
            pair_type: PairType::Mux1,
            pair_rom_info_index: Some(IDX_CTRL_MT32_V1_04_A),
        },
        RomInfo {
            file_size: 32768,
            blake3_digest: ctrl_mt32_v1_05_a_blake3,
            rom_type: RomType::Control,
            short_name: "ctrl_mt32_1_05_a",
            description: "MT-32 Control v1.05",
            pair_type: PairType::Mux0,
            pair_rom_info_index: Some(IDX_CTRL_MT32_V1_05_B),
        },
        RomInfo {
            file_size: 32768,
            blake3_digest: ctrl_mt32_v1_05_b_blake3,
            rom_type: RomType::Control,
            short_name: "ctrl_mt32_1_05_b",
            description: "MT-32 Control v1.05",
            pair_type: PairType::Mux1,
            pair_rom_info_index: Some(IDX_CTRL_MT32_V1_05_A),
        },
        RomInfo {
            file_size: 32768,
            blake3_digest: ctrl_mt32_v1_06_a_blake3,
            rom_type: RomType::Control,
            short_name: "ctrl_mt32_1_06_a",
            description: "MT-32 Control v1.06",
            pair_type: PairType::Mux0,
            pair_rom_info_index: Some(IDX_CTRL_MT32_V1_06_B),
        },
        RomInfo {
            file_size: 32768,
            blake3_digest: ctrl_mt32_v1_06_b_blake3,
            rom_type: RomType::Control,
            short_name: "ctrl_mt32_1_06_b",
            description: "MT-32 Control v1.06",
            pair_type: PairType::Mux1,
            pair_rom_info_index: Some(IDX_CTRL_MT32_V1_06_A),
        },
        RomInfo {
            file_size: 32768,
            blake3_digest: ctrl_mt32_v1_07_a_blake3,
            rom_type: RomType::Control,
            short_name: "ctrl_mt32_1_07_a",
            description: "MT-32 Control v1.07",
            pair_type: PairType::Mux0,
            pair_rom_info_index: Some(IDX_CTRL_MT32_V1_07_B),
        },
        RomInfo {
            file_size: 32768,
            blake3_digest: ctrl_mt32_v1_07_b_blake3,
            rom_type: RomType::Control,
            short_name: "ctrl_mt32_1_07_b",
            description: "MT-32 Control v1.07",
            pair_type: PairType::Mux1,
            pair_rom_info_index: Some(IDX_CTRL_MT32_V1_07_A),
        },
        RomInfo {
            file_size: 32768,
            blake3_digest: ctrl_mt32_bluer_a_blake3,
            rom_type: RomType::Control,
            short_name: "ctrl_mt32_bluer_a",
            description: "MT-32 Control BlueRidge",
            pair_type: PairType::Mux0,
            pair_rom_info_index: Some(IDX_CTRL_MT32_BLUER_B),
        },
        RomInfo {
            file_size: 32768,
            blake3_digest: ctrl_mt32_bluer_b_blake3,
            rom_type: RomType::Control,
            short_name: "ctrl_mt32_bluer_b",
            description: "MT-32 Control BlueRidge",
            pair_type: PairType::Mux1,
            pair_rom_info_index: Some(IDX_CTRL_MT32_BLUER_A),
        },
        RomInfo {
            file_size: 262144,
            blake3_digest: pcm_mt32_l_blake3,
            rom_type: RomType::Pcm,
            short_name: "pcm_mt32_l",
            description: "MT-32 PCM ROM",
            pair_type: PairType::FirstHalf,
            pair_rom_info_index: Some(IDX_PCM_MT32_H),
        },
        RomInfo {
            file_size: 262144,
            blake3_digest: pcm_mt32_h_blake3,
            rom_type: RomType::Pcm,
            short_name: "pcm_mt32_h",
            description: "MT-32 PCM ROM",
            pair_type: PairType::SecondHalf,
            pair_rom_info_index: Some(IDX_PCM_MT32_L),
        },
        RomInfo {
            file_size: 524288,
            blake3_digest: pcm_mt32_blake3,
            rom_type: RomType::Pcm,
            short_name: "pcm_cm32l_l",
            description: "CM-32L/CM-64/LAPC-I PCM ROM",
            pair_type: PairType::FirstHalf,
            pair_rom_info_index: Some(IDX_PCM_CM32L_H),
        },
        RomInfo {
            file_size: 524288,
            blake3_digest: pcm_cm32l_h_blake3,
            rom_type: RomType::Pcm,
            short_name: "pcm_cm32l_h",
            description: "CM-32L/CM-64/LAPC-I PCM ROM",
            pair_type: PairType::SecondHalf,
            pair_rom_info_index: Some(IDX_PCM_CM32L_L),
        },
    ]
}

pub(crate) fn get_rom_info(data: &[u8]) -> Option<&'static RomInfo> {
    get_rom_info_from_list(data, get_all_rom_infos())
}

pub(crate) fn get_rom_info_from_list<'a>(
    data: &[u8],
    rom_infos: &'a [RomInfo],
) -> Option<&'a RomInfo> {
    let file_size = data.len();
    let file_blake3_digest = blake3_digest(data);
    rom_infos.iter().find(|rom_info| {
        file_size == rom_info.file_size && file_blake3_digest == rom_info.blake3_digest
    })
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
        for (index, info) in all.iter().enumerate() {
            if let Some(pair_index) = info.pair_rom_info_index {
                assert!(
                    pair_index < ALL_ROM_COUNT,
                    "ROM {} has out-of-bounds pair index",
                    index
                );
                let pair_info = &all[pair_index];
                assert_eq!(
                    pair_info.pair_rom_info_index,
                    Some(index),
                    "ROM {} ({}) pair ROM {} ({}) does not point back",
                    index,
                    info.short_name,
                    pair_index,
                    pair_info.short_name,
                );
            }
        }
    }
}

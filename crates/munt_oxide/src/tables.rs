// Copyright (C) 2003, 2004, 2005, 2006, 2008, 2009 Dean Beeler, Jerome Fisher
// Copyright (C) 2011-2022 Dean Beeler, Jerome Fisher, Sergey V. Mikayev
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

/// Found from sample analysis.
pub(crate) static RES_AMP_DECAY_FACTOR_TABLE: [u8; 8] = [31, 16, 12, 8, 5, 3, 2, 1];

#[derive(Clone)]
pub(crate) struct Tables {
    /// CONFIRMED: This is used to convert several parameters to amp-modifying values in the TVA envelope:
    /// - PatchTemp.outputLevel
    /// - RhythmTemp.outlevel
    /// - PartialParam.tva.level
    /// - expression
    ///   It's used to determine how much to subtract from the amp envelope's target value
    pub(crate) level_to_amp_subtraction: [u8; 101],

    /// CONFIRMED: ...
    pub(crate) env_logarithmic_time: [u8; 256],

    /// CONFIRMED: Based on a table found by Mok in the MT-32 control ROM
    pub(crate) master_vol_to_amp_subtraction: [u8; 101],

    /// CONFIRMED:
    pub(crate) pulse_width_100_to_255: [u8; 101],

    /// The LA32 chip contains an exponent table inside. The table contains 12-bit integer values.
    /// The actual table size is 512 rows. The 9 higher bits of the fractional part of the argument are used as a lookup address.
    /// To improve the precision of computations, the lower bits are supposed to be used for interpolation as the LA32 chip also
    /// contains another 512-row table with inverted differences between the main table values.
    pub(crate) exp9: [u16; 512],
}

impl Tables {
    pub(crate) fn new() -> Self {
        let mut level_to_amp_subtraction = [0u8; 101];
        let mut env_logarithmic_time = [0u8; 256];
        let mut master_vol_to_amp_subtraction = [0u8; 101];
        let mut pulse_width_100_to_255 = [0u8; 101];
        let mut exp9 = [0u16; 512];
        for (lf, slot) in level_to_amp_subtraction.iter_mut().enumerate() {
            // CONFIRMED:KG: This matches a ROM table found by Mok
            let f_val = (2.0f32 - (lf as f32 + 1.0).log10()) * 128.0;
            let val = (f_val + 1.0) as i32;
            let val = if val > 255 { 255 } else { val };
            *slot = val as u8;
        }

        env_logarithmic_time[0] = 64;
        for (lf, slot) in env_logarithmic_time.iter_mut().enumerate().skip(1) {
            // CONFIRMED:KG: This matches a ROM table found by Mok
            *slot = (64.0f32 + (lf as f32).log2() * 8.0).ceil() as u8;
        }

        // CONFIRMED: Based on a table found by Mok in the MT-32 control ROM
        master_vol_to_amp_subtraction[0] = 255;
        for (master_vol, slot) in master_vol_to_amp_subtraction.iter_mut().enumerate().skip(1) {
            *slot = (106.31f32 - 16.0 * (master_vol as f32).log2()) as u8;
        }

        for (i, slot) in pulse_width_100_to_255.iter_mut().enumerate() {
            *slot = (i as f32 * 255.0 / 100.0 + 0.5) as u8;
        }

        // The LA32 chip contains an exponent table inside. The table contains 12-bit integer values.
        // The actual table size is 512 rows. The 9 higher bits of the fractional part of the argument are used as a lookup address.
        // To improve the precision of computations, the lower bits are supposed to be used for interpolation as the LA32 chip also
        // contains another 512-row table with inverted differences between the main table values.
        for (i, slot) in exp9.iter_mut().enumerate() {
            *slot = (8191.5f32 - f32::exp2(13.0 + !(i as i32) as f32 / 512.0)) as u16;
        }

        Tables {
            level_to_amp_subtraction,
            env_logarithmic_time,
            master_vol_to_amp_subtraction,
            pulse_width_100_to_255,
            exp9,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[rustfmt::skip]
    static  CPP_LEVEL_TO_AMP_SUBTRACTION: [u8; 101] = [
        255, 218, 195, 179, 167, 157, 148, 141, 134, 129, 123, 118, 114, 110, 106, 102, 99,
        96, 93, 90, 87, 85, 82, 80, 78, 75, 73, 71, 69, 67, 66, 64, 62, 60, 59, 57, 56, 54,
        53, 51, 50, 49, 47, 46, 45, 44, 42, 41, 40, 39, 38, 37, 36, 35, 34, 33, 32, 31, 30,
        29, 28, 27, 26, 25, 24, 24, 23, 22, 21, 20, 20, 19, 18, 17, 16, 16, 15, 14, 14, 13,
        12, 12, 11, 10, 10, 9, 8, 8, 7, 6, 6, 5, 5, 4, 3, 3, 2, 2, 1, 1, 0,
    ];

    #[rustfmt::skip]
    static CPP_ENV_LOGARITHMIC_TIME: [u8; 256] = [
        64, 64, 72, 77, 80, 83, 85, 87, 88, 90, 91, 92, 93, 94, 95, 96, 96, 97, 98, 98, 99,
        100, 100, 101, 101, 102, 102, 103, 103, 103, 104, 104, 104, 105, 105, 106, 106, 106,
        106, 107, 107, 107, 108, 108, 108, 108, 109, 109, 109, 109, 110, 110, 110, 110, 111,
        111, 111, 111, 111, 112, 112, 112, 112, 112, 112, 113, 113, 113, 113, 113, 114, 114,
        114, 114, 114, 114, 114, 115, 115, 115, 115, 115, 115, 116, 116, 116, 116, 116, 116,
        116, 116, 117, 117, 117, 117, 117, 117, 117, 117, 118, 118, 118, 118, 118, 118, 118,
        118, 118, 119, 119, 119, 119, 119, 119, 119, 119, 119, 119, 120, 120, 120, 120, 120,
        120, 120, 120, 120, 120, 120, 121, 121, 121, 121, 121, 121, 121, 121, 121, 121, 121,
        122, 122, 122, 122, 122, 122, 122, 122, 122, 122, 122, 122, 122, 123, 123, 123, 123,
        123, 123, 123, 123, 123, 123, 123, 123, 123, 124, 124, 124, 124, 124, 124, 124, 124,
        124, 124, 124, 124, 124, 124, 124, 124, 125, 125, 125, 125, 125, 125, 125, 125, 125,
        125, 125, 125, 125, 125, 125, 125, 126, 126, 126, 126, 126, 126, 126, 126, 126, 126,
        126, 126, 126, 126, 126, 126, 126, 126, 127, 127, 127, 127, 127, 127, 127, 127, 127,
        127, 127, 127, 127, 127, 127, 127, 127, 127, 127, 128, 128, 128, 128, 128, 128, 128,
        128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128,
    ];

    #[rustfmt::skip]
    static CPP_MASTER_VOL_TO_AMP_SUBTRACTION: [u8; 101] = [
        255, 106, 90, 80, 74, 69, 64, 61, 58, 55, 53, 50, 48, 47, 45, 43, 42, 40, 39, 38,
        37, 36, 34, 33, 32, 32, 31, 30, 29, 28, 27, 27, 26, 25, 24, 24, 23, 22, 22, 21, 21,
        20, 20, 19, 18, 18, 17, 17, 16, 16, 16, 15, 15, 14, 14, 13, 13, 12, 12, 12, 11, 11,
        11, 10, 10, 9, 9, 9, 8, 8, 8, 7, 7, 7, 6, 6, 6, 6, 5, 5, 5, 4, 4, 4, 4, 3, 3, 3,
        2, 2, 2, 2, 1, 1, 1, 1, 0, 0, 0, 0, 0,
    ];

    #[rustfmt::skip]
    static CPP_PULSE_WIDTH_100_TO_255: [u8; 101] = [
        0, 3, 5, 8, 10, 13, 15, 18, 20, 23, 26, 28, 31, 33, 36, 38, 41, 43, 46, 48, 51, 54,
        56, 59, 61, 64, 66, 69, 71, 74, 77, 79, 82, 84, 87, 89, 92, 94, 97, 99, 102, 105,
        107, 110, 112, 115, 117, 120, 122, 125, 128, 130, 133, 135, 138, 140, 143, 145, 148,
        150, 153, 156, 158, 161, 163, 166, 168, 171, 173, 176, 179, 181, 184, 186, 189, 191,
        194, 196, 199, 201, 204, 207, 209, 212, 214, 217, 219, 222, 224, 227, 230, 232, 235,
        237, 240, 242, 245, 247, 250, 252, 255,
    ];

    #[rustfmt::skip]
    static CPP_EXP9: [u16; 512] = [
        10, 21, 32, 43, 54, 65, 76, 87, 98, 109, 120, 131, 142, 153, 164, 175, 185, 196, 207,
        218, 229, 239, 250, 261, 272, 282, 293, 304, 314, 325, 336, 346, 357, 368, 378, 389,
        399, 410, 420, 431, 441, 452, 462, 473, 483, 494, 504, 514, 525, 535, 546, 556, 566,
        577, 587, 597, 607, 618, 628, 638, 648, 659, 669, 679, 689, 699, 709, 719, 730, 740,
        750, 760, 770, 780, 790, 800, 810, 820, 830, 840, 850, 860, 870, 880, 889, 899, 909,
        919, 929, 939, 949, 958, 968, 978, 988, 997, 1007, 1017, 1027, 1036, 1046, 1056, 1065,
        1075, 1085, 1094, 1104, 1113, 1123, 1132, 1142, 1152, 1161, 1171, 1180, 1190, 1199,
        1208, 1218, 1227, 1237, 1246, 1256, 1265, 1274, 1284, 1293, 1302, 1312, 1321, 1330,
        1340, 1349, 1358, 1367, 1377, 1386, 1395, 1404, 1413, 1423, 1432, 1441, 1450, 1459,
        1468, 1477, 1486, 1495, 1505, 1514, 1523, 1532, 1541, 1550, 1559, 1568, 1577, 1585,
        1594, 1603, 1612, 1621, 1630, 1639, 1648, 1657, 1665, 1674, 1683, 1692, 1701, 1710,
        1718, 1727, 1736, 1745, 1753, 1762, 1771, 1779, 1788, 1797, 1805, 1814, 1823, 1831,
        1840, 1848, 1857, 1866, 1874, 1883, 1891, 1900, 1908, 1917, 1925, 1934, 1942, 1951,
        1959, 1967, 1976, 1984, 1993, 2001, 2009, 2018, 2026, 2035, 2043, 2051, 2059, 2068,
        2076, 2084, 2093, 2101, 2109, 2117, 2126, 2134, 2142, 2150, 2158, 2166, 2175, 2183,
        2191, 2199, 2207, 2215, 2223, 2231, 2239, 2247, 2255, 2264, 2272, 2280, 2288, 2296,
        2304, 2311, 2319, 2327, 2335, 2343, 2351, 2359, 2367, 2375, 2383, 2391, 2398, 2406,
        2414, 2422, 2430, 2437, 2445, 2453, 2461, 2469, 2476, 2484, 2492, 2499, 2507, 2515,
        2523, 2530, 2538, 2545, 2553, 2561, 2568, 2576, 2584, 2591, 2599, 2606, 2614, 2621,
        2629, 2636, 2644, 2651, 2659, 2666, 2674, 2681, 2689, 2696, 2704, 2711, 2719, 2726,
        2733, 2741, 2748, 2755, 2763, 2770, 2778, 2785, 2792, 2799, 2807, 2814, 2821, 2829,
        2836, 2843, 2850, 2858, 2865, 2872, 2879, 2886, 2894, 2901, 2908, 2915, 2922, 2929,
        2936, 2943, 2951, 2958, 2965, 2972, 2979, 2986, 2993, 3000, 3007, 3014, 3021, 3028,
        3035, 3042, 3049, 3056, 3063, 3070, 3077, 3084, 3091, 3097, 3104, 3111, 3118, 3125,
        3132, 3139, 3146, 3152, 3159, 3166, 3173, 3180, 3186, 3193, 3200, 3207, 3213, 3220,
        3227, 3234, 3240, 3247, 3254, 3260, 3267, 3274, 3280, 3287, 3294, 3300, 3307, 3313,
        3320, 3327, 3333, 3340, 3346, 3353, 3359, 3366, 3372, 3379, 3386, 3392, 3399, 3405,
        3411, 3418, 3424, 3431, 3437, 3444, 3450, 3457, 3463, 3469, 3476, 3482, 3488, 3495,
        3501, 3508, 3514, 3520, 3527, 3533, 3539, 3545, 3552, 3558, 3564, 3571, 3577, 3583,
        3589, 3595, 3602, 3608, 3614, 3620, 3626, 3633, 3639, 3645, 3651, 3657, 3663, 3670,
        3676, 3682, 3688, 3694, 3700, 3706, 3712, 3718, 3724, 3730, 3736, 3742, 3748, 3754,
        3760, 3766, 3772, 3778, 3784, 3790, 3796, 3802, 3808, 3814, 3820, 3826, 3832, 3838,
        3844, 3849, 3855, 3861, 3867, 3873, 3879, 3885, 3890, 3896, 3902, 3908, 3914, 3919,
        3925, 3931, 3937, 3943, 3948, 3954, 3960, 3965, 3971, 3977, 3983, 3988, 3994, 4000,
        4005, 4011, 4017, 4022, 4028, 4034, 4039, 4045, 4050, 4056, 4062, 4067, 4073, 4078,
        4084, 4089, 4095,
    ];

    #[test]
    fn tables_match_cpp_reference() {
        let t = Tables::new();
        assert_eq!(t.level_to_amp_subtraction, CPP_LEVEL_TO_AMP_SUBTRACTION);
        assert_eq!(t.env_logarithmic_time, CPP_ENV_LOGARITHMIC_TIME);
        assert_eq!(
            t.master_vol_to_amp_subtraction,
            CPP_MASTER_VOL_TO_AMP_SUBTRACTION
        );
        assert_eq!(t.pulse_width_100_to_255, CPP_PULSE_WIDTH_100_TO_255);
        assert_eq!(t.exp9, CPP_EXP9);
    }
}

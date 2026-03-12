# ymfm_oxide 

Memory safe Rust reimplementation of [ymfm](https://github.com/aaronsgiles/ymfm) for the Yamaha FM synthesis chips.

## Ported chips

Not all chips are ported. We only ported the chips that were used by different sound cards for the PC-98.

| Chip   | Family | Features                                  | PC-98 sound card                 |
|--------|--------|-------------------------------------------|----------------------------------|
| YM2203 | OPN    | 3-ch FM + 3-ch SSG                        | PC-9801-26K                      |
| YM2608 | OPNA   | 6-ch stereo FM + SSG + ADPCM-A + ADPCM-B  | PC-9801-86                       |
| YM3526 | OPL    | 9-ch mono FM                              | (YM3812 predecessor, for compat) |
| Y8950  | OPL    | 9-ch mono FM + ADPCM-B                    | Sound Orchestra-V                |
| YM3812 | OPL2   | 9-ch mono FM, 4 waveforms                 | Sound Orchestra                  |
| YMF262 | OPL3   | 18-ch 4-output FM, 8 waveforms, 4-op mode | PC-9801-118, Sound Blaster 16    |

## License

This project is licensed under [3-clause BSD](https://opensource.org/license/bsd-3-clause) license.

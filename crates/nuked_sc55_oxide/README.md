# Nuked SC-55

This is a port of the [Roland SC-55 emulator](https://github.com/nukeykt/Nuked-SC55), by nukeykt, to Rust.
It has been mended to be used as a library for the emulator.
Most code for the LCD and button handling has been removed.

Supported models:

- SC-55mk2/SC-155mk2 (v1.01 firmware is confirmed to work)
- SC-55mk1 (v1.0/v1.21/v2.0 firmwares are confirmed to work)
- CM-300/SCC-1 (v1.10/v1.20 firmwares are confirmed to work)
- SCC-1A
- SC-55st (v1.01)
- JV-880 (v1.0.0/v1.0.1)
- SCB-55/RLP-3194
- RLP-3237
- SC-155

## Acknowledgement

Special thanks to:

- nukeykt: the original C++ implementation.
- John McMaster: SC-55 PCM chip decap.
- org/ogamespec: deroute tool.
- SDL team.
- Wohlstand: linux/macos port.
- mattw.
- HardWareMan.
- giulioz: JV-880 support
- Cloudschatze.
- NikitaLita.
- Karmeck.

## License

Same as the original Nuked SC-55, this port can be distributed and used under the original MAME license (see LICENSE file).
Non-commercial license was chosen to prevent making and selling SC-55 emulation boxes using (or around) this code,
as well as preventing from using it in the commercial music production.

# Neetan (ねーたん)

An emulator for the PC-98 written in Rust and using a Vulkan graphics engine.

## Design rationale

neetan's main goal is to be able to run PC-98 exclusive games and software on modern hardware.
It aims to provide high accuracy emulation, especially for the emulated CPUs, while still providing a good default,
out of the box experience. The default requires no font file, sound files or any ROM files. We build our own font ROM
using open source fonts, provide HLE SASI and HLE BIOS implementations. Providing original font ROM files and bios ROM
files is supported, but should provide no additional benefit.

## Supported systems

Currently, we aim to support all 16-bit era DOS games and emulate them accurately for 3 idealized machine targets:

| Machine   | CPU      | RAM     | Extended RAM |
|-----------|----------|---------|--------------|
| PC-9801VM | V30      | 640 KiB | None         |
| PC-9801VX | 80286    | 640 KiB | 4 MiB        |
| PC-9801RA | 80386 DX | 640 KiB | 14 MiB       |

All machines support up to two floppy drives and upt to two SASI hard drives.

We also support the following sound cards:

* PC beeper
* PC-9801-28k
* PC-9801-86
* PC-9801-86 + PC-9801-28k combo

The default for the CLI is the PC-9801VX machine with the PC-9801-86 + PC-9801-28k combo soundboards.

This machine was release 1986, and it was well-supported until around 1996.
For older games, or games that targeted the VM standard, we included the VC30 based VM machine.
For newer games, or games that were very resource intensive, we included the RA machine.

## Planned features

* Simple runtime savestates.
* Changing floppy inside the emulator. 
* 256 KB ADPCM RAM option for PC-9801-86
* PC-9821 support
  * 486 DX CPU
  * 256 color graphics
  * IDE HDD
  * ATAPI CDROM
* Support for more sound cards

## FAQ

### Which ROM files do I need for this emulation?

You don't need any rom files. If you have the correct rom files, you CAN use them, but there is not a particular reason
to use them, since our HLE BIOS and HLE SASI BIOS is handling the BIOS.ROM part very well these days.
We also include a self created open source font ROM and also provide the tools to re-create / change it.
With these systems in place we are able to run th fast majority of PC-98 games and applications.

There are some BIOS extensions, mainly the sound API and LIO API that we currently haven't implemented, but outside
some odd BASIC based games, they should not be used by games, which interface with the hardware I/O port directly.

### How can I use my mouse?

In games that support a mouse, you first need to capture the mouse pointer via the right CTRL key. You can release
the mouse pointer by clicking the right CTRL key again.

### How do I rebind my keys?

Not yet implemented, sorry.

## Build requirements

* A recent rust compiler
* Vulkan SDK
* slang shader compiler (comes bundled with the Vulkan SDK) 

For mac users:

* MoltenVK

## Acknowledgement

Following projects provided references for our implementation and test vectors. They were invaluable for developing
neetan:

- [MAME](https://www.mamedev.org/) 
- [NP21W](https://simk98.github.io/np21w/)
- [undoc98](https://www.webtech.co.jp/company/doc/undocumented_mem/index.html)

We also ported the Yamaha OPN and OPL emulation from the amazing YMFM project to our own Rust port:

- [ymfm](https://github.com/aaronsgiles/ymfm)

## License

This project is licensed under [3-clause BSD](https://opensource.org/license/bsd-3-clause) license, which is favored by  the PC-98 emulation scene.

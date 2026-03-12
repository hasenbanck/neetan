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
* PC-9801-26k
* PC-9801-86
* PC-9801-86 + PC-9801-26k combo

The default for the CLI is the PC-9801VX machine with the PC-9801-86 + PC-9801-26k combo soundboards.

This machine was release 1986, and it was well-supported until around 1996.
For older games, or games that targeted the VM standard, we included the V30 based VM machine.
For newer games, or games that were very resource intensive, we included the RA machine.

## Usage

```bash
neetan [OPTIONS]
neetan <COMMAND>
```

### Options

| Option                    | Description                                | Default  |
|---------------------------|--------------------------------------------|----------|
| `-c, --config <PATH>`     | Load configuration from file               | —        |
| `--machine <TYPE>`        | Machine type: `VM`, `VX`, `RA`             | `VX`     |
| `--fdd1 <PATH>`           | Floppy disk image for drive 1 (repeatable) | —        |
| `--fdd2 <PATH>`           | Floppy disk image for drive 2 (repeatable) | —        |
| `--hdd1 <PATH>`           | Hard disk image for SASI drive 1           | —        |
| `--hdd2 <PATH>`           | Hard disk image for SASI drive 2           | —        |
| `--audio-volume <FLOAT>`  | Audio volume 0.0–1.0                       | `1.0`    |
| `--aspect-mode <MODE>`    | Display aspect mode: `4:3` or `1:1`        | `4:3`    |
| `--bios-rom <PATH>`       | Path to BIOS ROM file                      | HLE BIOS |
| `--font-rom <PATH>`       | Path to font ROM file                      | Built-in |
| `--soundboard <TYPE>`     | Sound board: `none`, `26k`, `86`, `86+26k` | `86+26k` |
| `--printer <PATH>`        | Output file for printer (must exist)       | —        |
| `-h, --help`              | Print help                                 | —        |
| `-V, --version`           | Print version                              | —        |

### Commands

`create-fdd <PATH> [OPTIONS]` — Create an empty floppy disk image (D88 format).

| Option          | Description                         | Default |
|-----------------|-------------------------------------|---------|
| `--type <TYPE>` | `2hd` (1232 KiB) or `2dd` (640 KiB) | `2hd`   |

`create-hdd <PATH> [OPTIONS]` — Create an empty hard disk image (HDI format).

| Option          | Description                              | Default |
|-----------------|------------------------------------------|---------|
| `--type <TYPE>` | `5`, `10`, `15`, `20`, `30`, or `40` MiB | `40`    |

### Configuration file

Instead of passing all options on the command line, you can use a configuration file with `-c`:

```bash
neetan -c my_game.cfg
```

The file uses a simple `key = value` format. Lines starting with `#` are comments.

```ini
# Example configuration
machine = RA
soundboard = 86+26k
audio-volume = 0.8
aspect-mode = 4:3
fdd1 = /path/to/disk_a.d88
fdd1 = /path/to/disk_b.d88
fdd2 = /path/to/data.d88
hdd1 = /path/to/harddrive.hdi
```

Command-line arguments override values from the configuration file.

### Emulator controls

| Key        | Action                           |
|------------|----------------------------------|
| Right Ctrl | Toggle mouse capture             |
| GUI + F9   | Open floppy selector for drive 1 |
| GUI + F10  | Open floppy selector for drive 2 |

(GUI is the Windows / Command key.)

### Supported floppy disk image formats

| Format  | Extensions                     | Writable | Description                                        |
|---------|--------------------------------|----------|----------------------------------------------------|
| D88     | `.d88`, `.d98`, `.88d`, `.98d` | Yes      | Standard PC-98 disk image with per-sector metadata |
| HDM     | `.hdm`                         | No       | Headerless raw sector image (2HD only)             |
| NFD     | `.nfd`                         | No       | T98Next format with per-sector metadata            |

Only D88 images preserve modifications written by the emulated software. HDM and NFD images are currently read-only.

## Multiple floppy disk images

Many PC-98 games ship on multiple floppy disks and ask you to swap disks during gameplay.
neetan handles this by letting you assign several disk images to each drive up front, then swap
between them at runtime.

### Providing multiple disks

Use the `--fdd1` / `--fdd2` flags more than once to register all disks for a drive:

```bash
neetan --fdd1 floppy_disk1.d88 --fdd1 floppy_disk2.d88 --fdd1 floppy_disk3.d88
```

Or equivalently in a configuration file:

```ini
fdd1 = floppy_disk1.d88
fdd1 = floppy_disk2.d88
fdd1 = floppy_disk3.d88
```

The first image in each list is automatically inserted at startup.

### Swapping disks at runtime

Press **GUI + F9** (drive 1) or **GUI + F10** (drive 2) to open the floppy selector.

## Planned features

* Simple runtime savestates.
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
to use them, since our HLE BIOS and HLE SASI BIOS has very good compatibility.
We also include a self created open source font ROM and also provide the tools to re-create / change it.
With these systems in place we are able to run th fast majority of PC-98 games and applications.

There are some BIOS extensions, mainly the sound API and LIO API that we currently haven't implemented, but outside
some odd BASIC based games, they should not be used by games, which interface with the hardware I/O port directly.

### How can I use my mouse?

In games that support a mouse, you first need to capture the mouse pointer via the right CTRL key. You can release
the mouse pointer by clicking the right CTRL key again.

### How do I rebind my keys?

Not yet implemented, sorry.

### 日本語も分かりますか？

もちろん！IssueやPRの作成には日本語をご利用いただけますが、ソースコードのコメントについては英語での記述を推奨しております。

## Build requirements

* [Rust 1.94](https://rustup.rs/)
* [Vulkan SDK](https://vulkan.lunarg.com/sdk/home) 
* [SDL3](https://github.com/libsdl-org/SDL) (See [sdl3_sys descriptio](https://docs.rs/sdl3-sys/latest/sdl3_sys/#usage))
* [slang](https://github.com/shader-slang/slang/) (comes normally bundled with the Vulkan SDK) 

For Mac users:

* [MoltenVK](https://github.com/KhronosGroup/MoltenVK)

## Acknowledgement

Following projects provided references for our implementation and test vectors. They were invaluable for developing
neetan:

- [MAME](https://www.mamedev.org/) 
- [NP21W](https://simk98.github.io/np21w/)
- [undoc98](https://www.webtech.co.jp/company/doc/undocumented_mem/index.html)

We also ported the Yamaha OPN and OPL emulation from the amazing YMFM project to our own Rust port:

- [ymfm](https://github.com/aaronsgiles/ymfm)

## License

This project is licensed under [3-clause BSD](https://opensource.org/license/bsd-3-clause) license.

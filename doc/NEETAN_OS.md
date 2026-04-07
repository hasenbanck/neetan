# NEETAN OS Specification

NEETAN OS is a High-Level Emulation (HLE) DOS replacement for the Neetan emulator. It activates
when no bootable media is found, replacing the current "NO SYSTEM MEDIA FOUND" halt with a fully
functional MS-DOS 6.20-compatible environment.

All OS logic runs in Rust on the host side. Only the DOS data structures that programs expect to
find in conventional memory are placed in emulated RAM. This gives user programs the maximum
possible conventional memory (~636 KB free out of 640 KB).

## 1. Architecture

### 1.1 Trampoline Mechanism

NEETAN OS reuses the existing BIOS HLE trap mechanism. The BIOS ROM stub (`utils/bios/bios.asm`)
already provides interrupt handler stubs that:

1. Push AX and DX (clobbered by the trap sequence)
2. Write a vector number byte to trap port 0x07F0
3. Execute IRET

The emulator yields on the OUT instruction, restores AX/DX from the stack, dispatches the
appropriate Rust handler, then resumes the CPU to execute the IRET.

For DOS, the same pattern is extended to INT 20h-2Fh and INT 33h. New stubs are added to
`bios.asm` using the existing `hle_stub` macro:

```nasm
int_20h_handler:    hle_stub 0x20     ; DOS: Terminate Program
int_21h_handler:    hle_stub 0x21     ; DOS: Function Dispatch
int_22h_handler:    hle_stub 0x22     ; DOS: Terminate Address (default)
int_23h_handler:    hle_stub 0x23     ; DOS: Ctrl-Break Handler (default)
int_24h_handler:    hle_stub 0x24     ; DOS: Critical Error Handler (default)
int_25h_handler:    hle_stub 0x25     ; DOS: Absolute Disk Read
int_26h_handler:    hle_stub 0x26     ; DOS: Absolute Disk Write
int_27h_handler:    hle_stub 0x27     ; DOS: Terminate and Stay Resident (old)
int_28h_handler:    hle_stub 0x28     ; DOS: Idle
int_29h_handler:    hle_stub 0x29     ; DOS: Fast Console Output
int_2ah_handler:    hle_stub 0x2A     ; DOS: Network / Critical Section
int_2fh_handler:    hle_stub 0x2F     ; DOS: Multiplex
int_33h_handler:    hle_stub 0x33     ; Mouse Driver
int_dch_handler:    hle_stub 0xDC     ; NEC DOS Extension (IO.SYS)
```

These entries are appended to the vector table in `bios.asm`. The IVT slots for INT 22h, 23h,
and 24h are per-process vectors stored in the PSP; the ROM stubs serve as the default handlers.

In `crates/machine/src/bus/bios.rs`, the `execute_bios_hle()` match statement gains new arms
that delegate to the OS crate:

```
0x20..=0x2F | 0x33 | 0xDC => self.os.dispatch(vector, cpu, ...)
```

The `os` field is an `Option<NeetanOs>` on `Pc9801Bus`. It is `Some` when NEETAN OS is active
and `None` when booting from real media (in which case these vectors either pass through to
whatever the boot sector installed or hit the `iret_stub`).

### 1.2 Bootstrap Sequence

When `hle_bootstrap()` exhausts all boot devices without finding bootable media, instead of
writing "NO SYSTEM MEDIA FOUND" and halting, it:

1. Creates and initializes a `NeetanOs` instance in the `os` field
2. Calls `os.boot()` which:
   a. Writes the DOS data structures into emulated RAM (SYSVARS, SFT, CDS, DPB, MCB chain, etc.)
   b. Mounts available drives (floppies, HDDs, CD-ROM, virtual Z:)
   c. Parses CONFIG.SYS if present on any drive
   d. Creates the COMMAND.COM process (environment block + PSP + code stub)
   e. Executes AUTOEXEC.BAT if present
3. Rewrites the IRET frame to transfer control to the COMMAND.COM entry point

### 1.3 Shell Execution Model

The COMMAND.COM process has a tiny x86 code loop placed at PSP:0100h:

```nasm
loop:
    mov  ah, 0FFh       ; Reserved function: shell prompt/command cycle
    int  21h
    jmp  loop
```

When INT 21h sees AH=FFh, the Rust shell handler takes over:

1. Display the prompt (e.g., `A:¥>`)
2. Read an input line with history support (up/down arrow keys)
3. Parse the command line
4. If the command is a shell built-in (CD, internal commands), execute it directly
5. If the command is external, perform EXEC (INT 21h/4Bh) which creates a child process
6. When the child terminates, control returns here for the next command

This keeps the CPU executing real x86 code while all work happens in Rust, matching the
BIOS HLE pattern.

## 2. Memory Layout

### 2.1 Conventional Memory Map

```
0x00000-0x003FF  IVT (1024 bytes)                    [managed by BIOS HLE]
0x00400-0x005FF  BIOS Data Area (512 bytes)          [managed by BIOS HLE]
0x00600-0x00BFF  IO.SYS Work Area + DOS Data (~1.5 KB)
  0x00600         SYSVARS / List of Lists
  0x00640         NUL device header (18 bytes, embedded in SYSVARS, start of device chain)
  0x00660         Device headers (CON, CLOCK, $AID#NEC, MS$KANJI, etc.)
  0x00680         SFT header + initial file entries (5 standard handles)
  0x00800         Current Directory Structure (CDS) array
  0x00A00         Disk Parameter Block (DPB) chain
  0x00B00         Disk buffer header + one buffer
  0x00B80         InDOS flag, critical error flag
  0x00B90         FCB-SFT header
0x00C00-0x00C0F  First MCB (sentinel, marks start of allocatable memory)
0x00C10-0x00CFF  COMMAND.COM environment block (240 bytes)
0x00D00-0x00D0F  MCB for COMMAND.COM PSP
0x00D10-0x00EFF  COMMAND.COM PSP (256 bytes) + code stub (16 bytes)
0x00F00-0x00F0F  Last MCB ('Z', free memory)
0x00F10-0x9FFFF  Free memory (~636 KB)
```

These addresses are approximate. The exact layout is determined at boot time based on the
number of active drives (affecting CDS and DPB sizes) and CONFIG.SYS settings (FILES=, BUFFERS=).

### 2.2 List of Lists (SYSVARS)

INT 21h AH=52h returns ES:BX pointing to SYSVARS. Programs that access this (including many
games and TSRs) expect the standard MS-DOS 3.1+ layout:

| Offset | Size  | Field                                         |
|--------|-------|-----------------------------------------------|
| -0x02  | WORD  | Segment of first MCB                          |
| +0x00  | DWORD | Far pointer to first DPB                      |
| +0x04  | DWORD | Far pointer to first system file table        |
| +0x08  | DWORD | Far pointer to CLOCK device header            |
| +0x0C  | DWORD | Far pointer to CON device header              |
| +0x10  | WORD  | Maximum bytes per sector (typically 512/1024) |
| +0x12  | DWORD | Far pointer to first disk buffer              |
| +0x16  | DWORD | Far pointer to CDS array                      |
| +0x1A  | DWORD | Far pointer to FCB-SFT                        |
| +0x1E  | WORD  | Number of protected FCBs                      |
| +0x20  | BYTE  | Number of block devices                       |
| +0x21  | BYTE  | LASTDRIVE value (default 26)                  |
| +0x22  | 18B   | NUL device header (start of device chain)     |
|        |       | NEC DOS 6.20 chain (verified): NUL -> (IO.SYS |
|        |       | internal) -> $AID#NEC -> CON -> MS$KANJI ->   |
|        |       | block device drivers. CLOCK device is NOT in  |
|        |       | the chain; it is only referenced by the       |
|        |       | SYSVARS+0x08 pointer. No PRN or AUX as named  |
|        |       | character devices; handles 3/4 map in the SFT |
| +0x34  | WORD  | Number of JOIN'ed drives (DOS 4.0+)           |
| +0x37  | DWORD | Pointer to SETVER list (DOS 5.0+)             |
| +0x3F  | WORD  | BUFFERS= value                                |
| +0x41  | WORD  | Number of lookahead buffers                   |
| +0x43  | BYTE  | Boot drive (1=A, 2=B, ...)                    |
| +0x44  | BYTE  | 386+ flag (01h = DWORD moves used)            |
| +0x45  | WORD  | Extended memory size in KB                    |

### 2.3 IO.SYS Work Area (Segment 0060h)

NEC's IO.SYS reserves segment 0060h (linear address 0x00600) as an internal work area.
Programs read and write these fields directly. NEETAN OS must populate the critical fields
at boot since it replaces IO.SYS. The work area overlaps with the DOS data structures
above (SYSVARS lives at the start of segment 0060h).

Fields that NEETAN OS must initialize:

| Address          | Size     | Content                                                       |
|------------------|----------|---------------------------------------------------------------|
| 0060:0020h       | WORD     | MS-DOS product number (returned by INT DCh CL=12h)            |
| 0060:0022h       | BYTE     | Internal revision number (returned by INT DCh CL=15h)         |
| 0060:0030h       | BYTE     | EMM.SYS B0000h bank auto-switch flag (00h default)            |
| 0060:0031h       | BYTE     | Extended memory size in 128KB units                           |
| 0060:0038h       | BYTE     | Logical FD drive duplicate assignment (00h=1:1, FFh=2:1)      |
| 0060:0068h       | WORD     | RS-232C ch0 AUX protocol (copied from mem switch 1 at boot)   |
| 0060:006Ch-007Bh | 16 BYTES | A:-P: DA/UA mapping list (returned by INT DCh CL=13h)         |
| 0060:008Ah       | BYTE     | Kanji/Graph mode (00h=graphic, 01h=Shift-JIS kanji)           |
| 0060:008Bh       | BYTE     | Graph mode display character (20h in kanji, 67h in graph)     |
| 0060:008Ch       | BYTE     | Shift-function display char (20h normal, 2Ah when shown)      |
| 0060:00A4h       | BYTE     | STOP key interrupt re-entry prevention (00h)                  |
| 0060:00B4h       | BYTE     | INT DCh processing flag (00h=idle)                            |
| 0060:0106h       | BYTE     | Special input mode (00h=normal)                               |
| 0060:0107h       | BYTE     | CTRL+P/N printer echo mode (00h=off)                          |
| 0060:010Ch       | BYTE     | CTRL+XFER/NFER and Fn softkey flags                           |
| 0060:0110h       | BYTE     | Cursor Y position (0-based from top)                          |
| 0060:0111h       | BYTE     | Function key display state (00h=hide, 01h=show, 02h=shift-fn) |
| 0060:0112h       | BYTE     | Scroll range lower limit (row number)                         |
| 0060:0113h       | BYTE     | Screen line count (00h=20-line, 01h=25-line)                  |
| 0060:0114h       | BYTE     | Clear attribute (E1h default; 81h for green text)             |
| 0060:0115h       | BYTE     | Kanji high byte flag (00h=normal, 01h=awaiting low byte)      |
| 0060:0116h       | BYTE     | Kanji high byte storage (first byte of kanji pair)            |
| 0060:0117h       | BYTE     | Line wrap flag (00h=wrap at 80, 01h=truncate; ESC[?7h/l)      |
| 0060:0118h       | BYTE     | Scroll speed (00h=normal, 01h=slow; CTRL+f9 toggle)           |
| 0060:0119h       | BYTE     | Clear character (normally 20h/space)                          |
| 0060:011Bh       | BYTE     | Cursor visibility (00h=hidden ESC[>5h, 01h=shown ESC[>5l)     |
| 0060:011Ch       | BYTE     | Cursor X position (0-based from left)                         |
| 0060:011Dh       | BYTE     | Display attribute (current text attribute from ESC[...m)      |
| 0060:011Eh       | BYTE     | Scroll range upper limit (row number, 0-based)                |
| 0060:011Fh       | WORD     | Scroll wait value (0001h=normal, E000h=slow)                  |
| 0060:0126h       | BYTE     | Saved cursor Y (ESC[s)                                        |
| 0060:0127h       | BYTE     | Saved cursor X (ESC[s)                                        |
| 0060:012Bh       | BYTE     | Saved cursor attribute (ESC[s)                                |
| 0060:013Bh       | BYTE     | Logical FD duplicate (duplicate of 0038h)                     |
| 0060:013Ch       | WORD     | Extended attribute mode display attribute                     |
| 0060:013Eh       | WORD     | Extended attribute mode clear attribute                       |
| 0060:0136h       | BYTE     | Last accessed drive unit number                               |
| 0060:05D6h       | WORD     | Extended attribute mode (0000h=PC, 0001h=EGH)                 |
| 0060:05D8h       | WORD     | Text mode (0000h=25-line gapped, 0001h=20/25 default)         |
| 0060:2820h       | DWORD    | Pointer to DA/UA list data area                               |
| 0060:2C86h       | 52 BYTES | A:-Z: extended DA/UA (attribute+DA/UA pairs, 2B each)         |

The DA/UA mapping at 0060:006Ch is critical. Each byte maps a drive letter (A:=index 0
through P:=index 15) to a Device Address / Unit Address byte:
- 0x90-0x93: 1MB FDD (2HD) units 0-3
- 0x70-0x73: 640KB FDD (2DD) units 0-3
- 0x80-0x83: SASI/IDE HDD units 0-3
- 0xA0-0xA7: SCSI units 0-7
- 0x68-0x6B: BRANCH 4670 virtual drive
- 0xD0: ROM drive
- 0xD1: PCMCIA memory card drive
- 0xE0: RAM disk (RAMDISK.SYS)
- 0xF0-0xF3: External 1MB FDD (1MB-only mode)
- 0x00: no drive assigned

An extended DA/UA table exists at 0060:2C86h (52 bytes). It contains attribute+DA/UA
pairs for all 26 drives A:-Z: (2 bytes per drive: attribute at even offset, DA/UA at
odd offset). This is the data returned at offsets +1Ah through +4Dh by INT DCh CL=13h.
The attribute byte encodes: bit 7 = dual-drive assignment, bit 1 = 32-bit sector
addressing (BPB offset 13h is 0), bit 0 = MO device.

The RS-232C AUX protocol word at 0060:0068h has this bit layout (programs parse
individual bits):

| Bits  | Field                                                                                                    |
|-------|----------------------------------------------------------------------------------------------------------|
| 15-12 | Reserved                                                                                                 |
| 11-8  | Baud rate: 1001=19200, 1000=9600, 0111=4800, 0110=2400, 0101=1200, 0100=600, 0011=300, 0010=150, 0001=75 |
| 7-6   | Stop bits: 01=1 bit, 11=2 bits                                                                           |
| 5     | Parity type: 1=even, 0=odd                                                                               |
| 4     | Parity enable: 1=on, 0=off                                                                               |
| 3-2   | Data bits: 10=7 bits, 11=8 bits                                                                          |
| 1     | Duplex: 0=full, 1=half                                                                                   |
| 0     | X-flow control: 1=on, 0=off                                                                              |

### 2.4 Memory Control Blocks (MCB)

Each MCB is a 16-byte paragraph header preceding the memory block it describes:

| Offset | Size | Field                                           |
|--------|------|-------------------------------------------------|
| 0x00   | BYTE | 'M' (0x4D) = more blocks, 'Z' (0x5A) = last     |
| 0x01   | WORD | Owner PSP segment (0x0000 = free, 0x0008 = DOS) |
| 0x03   | WORD | Size in paragraphs (excluding this header)      |
| 0x05   | 3B   | Reserved                                        |
| 0x08   | 8B   | Owner name (DOS 4.0+, blank-padded)             |

The MCB chain is walked by INT 21h functions 48h (allocate), 49h (free), and 4Ah (resize).
The Rust OS manipulates MCBs directly in `memory.state.ram[]`.

### 2.5 Program Segment Prefix (PSP)

Every running process has a 256-byte PSP at a paragraph boundary in emulated RAM:

| Offset | Size  | Field                                                              |
|--------|-------|--------------------------------------------------------------------|
| 0x00   | 2B    | INT 20h instruction (CD 20)                                        |
| 0x02   | WORD  | Segment of memory top                                              |
| 0x05   | 5B    | Far call to INT 21h dispatcher                                     |
| 0x0A   | DWORD | Saved INT 22h (terminate address)                                  |
| 0x0E   | DWORD | Saved INT 23h (Ctrl-Break handler)                                 |
| 0x12   | DWORD | Saved INT 24h (critical error handler)                             |
| 0x16   | WORD  | Parent PSP segment                                                 |
| 0x18   | 20B   | Job File Table (handle-to-SFT mapping)                             |
| 0x2C   | WORD  | Environment segment                                                |
| 0x32   | WORD  | Handle table size (default 20)                                     |
| 0x34   | DWORD | Far pointer to handle table (default PSP:0018h)                    |
| 0x50   | 3B    | INT 21h / RETF stub                                                |
| 0x5C   | 36B   | Default FCB #1 (filled by EXEC from 1st argument)                  |
| 0x6C   | 20B   | Default FCB #2 (filled by EXEC from 2nd argument, overlaps FCB #1) |
| 0x80   | BYTE  | Command tail length                                                |
| 0x81   | 127B  | Command tail string                                                |

The PSP must live in emulated RAM because user programs read and write it directly
(particularly the command tail at 0x80 and the environment segment at 0x2C).

### 2.6 Environment Block

Each process has an environment block at the segment stored in PSP+0x2C. It contains
null-terminated KEY=VALUE strings followed by a double-null terminator, then a WORD count
(0x0001) and the program pathname.

Note: on real NEC MS-DOS 6.20, COMMAND.COM does NOT set up the WORD count + pathname
after its own environment block. The area after the double-null contains uninitialized
MCB data (count != 0x0001). Child processes launched via INT 21h/4Bh (EXEC) receive a
valid program name from DOS. Our HLE OS sets the program name for all processes including
the root COMMAND.COM stub, for maximum compatibility with programs that read it.

Default COMMAND.COM environment:
```
COMSPEC=Z:¥COMMAND.COM\0
PATH=Z:¥;A:¥;B:¥;C:¥;\0
PROMPT=$P$G\0
\0
\x01\x00Z:¥COMMAND.COM\0
```

## 3. Drive System

### 3.1 Drive Letter Assignment

Drive letters are assigned dynamically at boot based on connected devices, following PC-98
MS-DOS conventions. On real PC-98 MS-DOS, the boot drive is always assigned A: regardless
of device type (if booting from HDD, the hard disk becomes A: and floppies follow). Since
NEETAN OS activates only when no bootable media is found, there is no boot drive to
prioritize, so it follows the floppy-first convention:

1. Floppy drives are assigned first: A:, B:, C:, D: (up to 4, based on equipped drives)
2. Hard disk partitions follow: next available letters (typically C: or E: depending on floppy count)
3. CD-ROM: assigned via MSCDEX (default Q: following PC-98 convention)
4. Z: is always the virtual read-only OS drive

The standard PC-98 configuration with 2 floppy drives and 1 HDD yields:
A:=FD0, B:=FD1, C:=HDD0 partition 0.

The drive letter assignments must be reflected in the DA/UA mapping table at 0060:006Ch
(see section 2.3).

Each drive has a corresponding Current Directory Structure (CDS) entry and Disk Parameter
Block (DPB). The DPB contains the physical geometry and FAT parameters read from the
volume's BIOS Parameter Block (BPB).

DPB structure (DOS 4.0+ / NEC DOS 6.20, 33 bytes per entry):

| Offset | Size  | Field                                    |
|--------|-------|------------------------------------------|
| +0x00  | BYTE  | Drive number (0=A:, 1=B:, ...)           |
| +0x01  | BYTE  | Unit number within device driver          |
| +0x02  | WORD  | Bytes per sector                         |
| +0x04  | BYTE  | Sectors per cluster - 1 (cluster mask)   |
| +0x05  | BYTE  | Cluster-to-sector shift count            |
| +0x06  | WORD  | Number of reserved (boot) sectors        |
| +0x08  | BYTE  | Number of FATs                           |
| +0x09  | WORD  | Number of root directory entries          |
| +0x0B  | WORD  | First sector of first cluster (data area)|
| +0x0D  | WORD  | Highest cluster number + 1               |
| +0x0F  | WORD  | Sectors per FAT                          |
| +0x11  | WORD  | First sector of root directory            |
| +0x13  | DWORD | Far pointer to device header              |
| +0x17  | BYTE  | Media descriptor byte (>= 0xF0)         |
| +0x18  | BYTE  | Disk access flag (0xFF = not yet accessed)|
| +0x19  | DWORD | Far pointer to next DPB (FFFFh:FFFFh = end)|

Device addresses map to drives:
- DA 0x90-0x93: 1MB FDD (2HD) drives 0-3
- DA 0x70-0x73: 640KB FDD (2DD) drives 0-3
- DA 0x80-0x83: SASI/ESDI/IDE HDD drives 0-3
- DA 0xA0-0xA6: SCSI HD, MO, CD-ROM (by SCSI ID)
- DA 0xF0-0xF3: External 1MB FDD (1MB-only mode)
- DA 0xD0: ROM drive
- DA 0xD1: PCMCIA memory card drive
- DA 0xE0: RAM disk (RAMDISK.SYS)
- CD-ROM: via IDE ATAPI channel or SCSI (DA 0xA0+)

### 3.2 Virtual Z: Drive

The Z: drive is a read-only virtual filesystem that exists entirely in Rust. It contains
the built-in OS commands as files (DIR.COM, COPY.COM, FORMAT.COM, etc.).

Implementation:
- The CDS entry for Z: has the network/substituted flag set, indicating a virtual drive
- FINDFIRST/FINDNEXT on Z: enumerates registered `Command` implementations as .COM files
- File attributes report read-only, archive; file sizes are nominal (1 byte each)
- Date/time stamps report the build date of the emulator
- EXEC (INT 21h/4Bh) targeting Z: files is intercepted entirely: no program is loaded into
  emulated RAM; instead, the corresponding Rust `Command::execute()` runs directly
- Read/write operations on Z: files return access denied (they exist only for DIR and EXEC)
- The current directory on Z: is always the root

### 3.3 FAT12/FAT16 Filesystem

The OS provides read/write access to FAT12 and FAT16 volumes on floppy and hard disk images.

FAT type detection follows the MS-DOS convention based on cluster count:
- Fewer than 4085 data clusters: FAT12
- 4085 to 65524 data clusters: FAT16

The filesystem module operates at the sector level. It does not depend on the device crate
directly; instead, the OS crate defines a `DiskIo` trait that the machine crate implements
using the actual device controllers:

```rust
pub trait DiskIo {
    /// Read sectors from a physical drive.
    /// drive_da: device address (0x90 for FD0, 0x80 for HDD0, etc.)
    /// lba: logical block address (0-based)
    /// count: number of sectors to read
    /// Returns sector data or an error code.
    fn read_sectors(&mut self, drive_da: u8, lba: u32, count: u32) -> Result<Vec<u8>, u8>;

    /// Write sectors to a physical drive.
    fn write_sectors(&mut self, drive_da: u8, lba: u32, data: &[u8]) -> Result<(), u8>;

    /// Get the sector size for a drive (typically 512 for HDD, 512 or 1024 for FDD).
    fn sector_size(&self, drive_da: u8) -> Option<u16>;

    /// Get total sector count for a drive.
    fn total_sectors(&self, drive_da: u8) -> Option<u32>;
}
```

For floppy access, the machine-crate implementation translates LBA to CHS using the BPB's
sectors-per-track and heads-per-cylinder values, then calls
`FloppyController::read_sector_data()` / `write_sector_data()`.

For HDD access, the implementation calls `HddImage::read_sector(lba)` / `write_sector(lba)`.

The FAT driver caches:
- The BPB (BIOS Parameter Block) from sector 0 of each mounted volume
- The FAT table (read on first access, flushed to disk on file close and disk reset)
- Directory sector buffers during enumeration

### 3.4 CD-ROM and MSCDEX

CD-ROM data track access is provided through MSCDEX (Microsoft CD-ROM Extensions), which
installs itself on INT 2Fh AH=15h.

#### Data Access

The CD-ROM data track is mapped to a drive letter (default Q:). MSCDEX provides:
- Drive letter assignment and verification
- Volume Table of Contents (VTOC) / ISO 9660 primary volume descriptor reading
- Directory entry lookup
- Sector reading via the device driver request interface

For data access, the OS reads sectors from the CdImage's data tracks using cooked (2048-byte)
sector reads and presents them through standard file I/O functions.

#### MSCDEX INT 2Fh Subfunctions

Programs discover and query MSCDEX through INT 2Fh AH=15h with these AL subfunctions:

| AX    | Function                   | Returns                                               |
|-------|----------------------------|-------------------------------------------------------|
| 1500h | Installation check         | BX = number of CD-ROM drives, CX = first drive letter |
| 150Bh | CD-ROM drive check         | AX = 0 if not CD-ROM, nonzero if CD-ROM               |
| 150Ch | Get MSCDEX version         | BH = major, BL = minor                                |
| 150Dh | Get CD-ROM drive letters   | Buffer at ES:BX filled with drive unit bytes          |
| 1510h | Send device driver request | ES:BX = device driver request header                  |

AX=1500h is the first call any CD-ROM-aware program makes. Without it, programs
cannot detect that MSCDEX is installed.

#### Audio Playback

CD-DA audio is controlled through MSCDEX's device driver request interface
(INT 2Fh AX=1510h). The caller places a device driver request header in emulated RAM at
ES:BX with a command code:

| Command | Function        | Request Data                           |
|---------|-----------------|----------------------------------------|
| 3       | IOCTL Input     | Control block for status queries       |
| 12      | IOCTL Output    | Control block for drive control        |
| 128     | Read Long       | Sector read (cooked 2048 / raw 2352)   |
| 131     | Seek            | Seek to sector position                |
| 132     | Play Audio      | Start/end sector addresses             |
| 133     | Stop Audio      | Stop current playback                  |
| 136     | Resume Audio    | Resume paused playback                 |

IOCTL Input control block codes relevant to audio:
- Code 6: Device status
- Code 10: Audio disk info (first track, last track, lead-out address)
- Code 11: Audio track info (start address, control flags for given track number)
- Code 12: Audio Q-channel info (current position during playback)
- Code 15: Audio status (paused, playing, or stopped)

#### CdAudioPlayer

Audio playback is handled by a `CdAudioPlayer` on `Pc9801Bus` that is shared between the
MSCDEX HLE path and the ATAPI LLE path (for games that access CD audio through direct
IDE ATAPI commands):

```rust
pub struct CdAudioPlayer {
    state: CdAudioState,      // Stopped, Playing, Paused
    start_lba: u32,           // First sector of play range
    end_lba: u32,             // Last sector of play range (exclusive)
    current_lba: u32,         // Current read position
    sector_buffer: [u8; 2352], // Current audio sector (16-bit stereo, 44100 Hz)
    buffer_offset: usize,     // Read position within current sector
}
```

When playing, the audio engine's sample generation mixes CD-DA PCM data from the
`CdAudioPlayer` into the output buffer alongside existing sound sources (OPN, OPNA, ADPCM,
etc.). Audio sectors are 2352 bytes of raw 16-bit signed little-endian stereo PCM at 44100 Hz
(588 stereo samples per sector, 75 sectors per second).

### 3.5 Shift-JIS Path Handling

All path parsing must be DBCS-aware. The byte 0x5C (backslash / path separator) appears
as the trail byte in many common Shift-JIS kanji characters:

- 表 (0x955C) "table/surface"
- ソ (0x835C) katakana "so"
- 十 (0x8F5C) "ten"
- 能 (0x945C) "ability"

When scanning a path string for 0x5C directory separators, the parser must skip over
DBCS lead+trail byte pairs. A byte is a DBCS lead byte if it falls in the ranges
0x81-0x9F or 0xE0-0xFC. If 0x5C follows a lead byte, it is the second byte of a kanji
character, not a path separator.

This affects every INT 21h function that accepts a path in DS:DX: file open (3Dh),
file create (3Ch), delete (41h), rename (56h), CHDIR (3Bh), MKDIR (39h), RMDIR (3Ah),
EXEC (4Bh), FINDFIRST (4Eh), and FCB filename parsing (29h).

### 3.6 HDD Partition Table

PC-98 hard disks use a proprietary partition scheme (not MBR):

- **Sector 0**: IPL (Initial Program Loader) boot code. Starts with a JMP instruction
  at byte 0, "IPL1" signature at offset 0x04, and 0xAA55 magic at the last two bytes
  of the sector (offset sector_size-2). Note: sector size varies by drive type (256
  bytes for old SASI drives, 512 bytes for IDE/SCSI).
- **Sector 1**: Partition table containing entries of 32 bytes each (up to 16 entries
  spanning one or more sectors depending on sector size).

Each 32-byte partition entry:

| Offset | Size | Field                                                 |
|--------|------|-------------------------------------------------------|
| 0x00   | 1    | mid -- type byte 0 (bit 7: bootable)                  |
| 0x01   | 1    | sid -- type byte 1 (bit 7: active, 0=sleep/hidden)    |
| 0x02   | 2    | Reserved padding                                      |
| 0x04   | 1    | IPL sector                                            |
| 0x05   | 1    | IPL head                                              |
| 0x06   | 2    | IPL cylinder (16-bit LE)                              |
| 0x08   | 1    | Data start sector                                     |
| 0x09   | 1    | Data start head                                       |
| 0x0A   | 2    | Data start cylinder (16-bit LE)                       |
| 0x0C   | 1    | End sector                                            |
| 0x0D   | 1    | End head                                              |
| 0x0E   | 2    | End cylinder (16-bit LE)                              |
| 0x10   | 16   | Partition name (ASCII/Shift-JIS, e.g., "MS-DOS 6.20") |

The mid byte encodes the OS type: 0x20-0x2F for DOS/Windows, 0x14 for BSD/PC-UX,
0x40 for Minix. The sid byte encodes the filesystem: 0x01=FAT12, 0x11=FAT16 <32MB,
0x21=FAT16 >=32MB, 0x31=NTFS, 0x61=FAT32.

To mount HDD partitions, NEETAN OS reads sector 1, iterates the 16 entries, and mounts
active DOS partitions (sid bit 7 set, mid in 0x20-0x2F range). The BPB is read from
each partition's boot sector (located via the data start CHS fields). The DPB for each
partition must account for the partition's starting LBA offset.

### 3.7 Standard Floppy Disk Geometries

PC-98 floppy formats differ from IBM PC. The BPB is read from sector 0 of each floppy.
Standard formats:

| Format      | Bytes/Sector | Tracks | Sectors/Track | Heads | Media | Capacity |
|-------------|--------------|--------|---------------|-------|-------|----------|
| 2HD (1.2MB) | 1024         | 77     | 8             | 2     | 0xFE  | 1232 KB  |
| 2DD (640KB) | 512          | 80     | 8             | 2     | 0xFE  | 640 KB   |
| 2DD (720KB) | 512          | 80     | 9             | 2     | 0xF9  | 720 KB   |

Note: SYSVARS max bytes per sector (+0x10) must be set to 1024 if any 2HD floppy is
mounted. DPB bytes-per-sector can legitimately hold 256 (old SASI drives) or 1024
(2HD floppies) -- values that IBM PC DOS never encounters.

## 4. Process Management

### 4.1 Program Loading

The EXEC function (INT 21h AH=4Bh) supports two program formats:

#### .COM Files

1. Find the largest available memory block via MCB chain
2. Allocate MCB for the process
3. Create PSP at the start of the allocated block
4. Load the entire file at PSP_segment:0100h
5. Set initial registers: CS=DS=ES=SS=PSP_segment, IP=0100h, SP=FFFEh
6. Push 0x0000 on stack (return address for RET -> PSP INT 20h)

#### .EXE (MZ) Files

1. Read and validate MZ header (signature 'MZ' or 'ZM' at offset 0-1)
2. Calculate load size from header (pages * 512 - header size)
3. Allocate memory: MINALLOC to MAXALLOC paragraphs (from header fields)
4. Create PSP at the start of the allocated block
5. Load the program image starting at PSP_segment + 0x10 (after PSP)
6. Apply segment relocations: for each relocation entry, add the load segment to the
   word at the specified offset
7. Set CS:IP and SS:SP from header fields, adjusted by load segment

#### Z: Drive Shortcut

When EXEC targets a file on the Z: drive, loading is skipped entirely. The OS identifies
the command by filename, creates a minimal process context, and executes the corresponding
Rust `Command::execute()` directly.

### 4.2 Process Stack and EXEC

The OS maintains a process stack for nested EXEC calls:

```rust
pub struct ProcessContext {
    psp_segment: u16,         // PSP segment of the suspended process
    return_cs: u16,           // Return address after child terminates
    return_ip: u16,
    return_ss: u16,
    return_sp: u16,
    saved_dta_seg: u16,       // Saved DTA address
    saved_dta_off: u16,
}
```

When EXEC creates a child process:
1. Push the current process context onto the process stack
2. Save the current INT 22h/23h/24h vectors from the IVT
3. Write the saved vectors into the new PSP at offsets 0x0A/0x0E/0x12
4. Set INT 22h to the caller's return address
5. Rewrite the IRET frame to transfer control to the child's entry point

### 4.3 Termination

When a process terminates (INT 20h, INT 21h/4Ch, or INT 27h for TSR):

1. Close all file handles owned by the process (JFT entries)
2. Free the process's MCB (except for TSR, which keeps it allocated)
3. Pop the parent's process context from the stack
4. Restore INT 22h/23h/24h from the terminated process's PSP
5. Store the return code (accessible via INT 21h/4Dh)
6. Transfer control to the parent's INT 22h address

For TSR (INT 21h/31h and INT 27h):
- The MCB is resized to the requested resident size but not freed
- The resident code remains in memory and may hook interrupts
- Control returns to the parent process

## 5. Console and Shell

### 5.1 Console I/O

Console output writes directly to text VRAM at 0xA0000, with cursor positioning managed
through BIOS INT 18h HLE. This ensures correct behavior with the GDC display controller:

- INT 21h AH=02h (Display Character): Writes character directly to text VRAM at the
  current cursor position, advances cursor with line wrap and scroll
- INT 21h AH=09h (Display String): Iterates '$'-terminated string, calling AH=02h per char
- INT 29h (Fast Console Output): Direct character output (AL = character), bypasses
  DOS checks

Console input reads from the PC-98 keyboard buffer (BIOS data area at 0x0502):
- INT 21h AH=01h/06h/07h/08h: Single character input
- INT 21h AH=0Ah: Buffered line input with editing

#### Scrolling

When the cursor reaches the bottom row of the text display (typically row 24 in 80x25
mode), the console scrolls:

1. Copy text VRAM rows 1-24 to rows 0-23 (each row = 160 bytes: 80 chars * 2 bytes)
2. Copy attribute VRAM rows 1-24 to rows 0-23 (at 0xA2000)
3. Clear the last row (fill with spaces + default attribute)
4. Place cursor at column 0 of the last row

This matches real DOS/BIOS scrolling behavior. The INT 18h HLE provides the
scroll-up function.

#### Path Display

On PC-98, the byte 0x5C (backslash in ASCII) is displayed as the yen sign by the
character generator ROM. No special handling is needed in the OS: paths use 0x5C as the
separator, and the hardware displays it as the yen sign. This is consistent with how
MS-DOS behaves on PC-98.

#### Native ESC Sequence Support

The PC-98 console driver in IO.SYS natively supports ESC (0x1B) sequences without
requiring ANSI.SYS. Programs output ESC sequences through INT 21h AH=02h or INT 29h
and expect them to be interpreted. NEETAN OS must process these in its console output
path.

The console driver maintains a state machine: when ESC (0x1B) is received, subsequent
characters are buffered until a complete sequence is recognized. Supported sequences:

| Sequence     | Function                                     |
|--------------|----------------------------------------------|
| ESC[row;colH | Set cursor position (1-based row and column) |
| ESC[nA       | Cursor up n rows                             |
| ESC[nB       | Cursor down n rows                           |
| ESC[nC       | Cursor right n columns                       |
| ESC[nD       | Cursor left n columns                        |
| ESC[s        | Save cursor position                         |
| ESC[u        | Restore cursor position                      |
| ESC[2J       | Clear entire screen                          |
| ESC[K        | Clear from cursor to end of line             |
| ESC[1K       | Clear from start of line to cursor           |
| ESC[2K       | Clear entire line                            |
| ESC[nL       | Insert n lines (scroll down)                 |
| ESC[nM       | Delete n lines (scroll up)                   |
| ESC[>1h      | Hide function key display row                |
| ESC[>1l      | Show function key display row                |
| ESC[>3h      | Set 20-line text mode                        |
| ESC[>3l      | Set 25-line text mode                        |
| ESC[>5h      | Hide cursor                                  |
| ESC[>5l      | Show cursor                                  |
| ESC[?7h      | Enable line wrap at column 80 (default)      |
| ESC[?7l      | Disable line wrap (truncate at column 80)    |
| ESC)0        | Set Shift-JIS kanji display mode             |
| ESC)3        | Set graphic character display mode           |

The function key display state (ESC[>1h/l) is reflected in 0060:0111h. The cursor
visibility (ESC[>5h/l) is reflected in 0060:011Bh. The line wrap flag (ESC[?7h/l)
is reflected in 0060:0117h. The kanji/graph mode (ESC)0/ESC)3) is reflected in
0060:008Ah. See section 2.3 for the IO.SYS work area.

### 5.2 Command Shell

The COMMAND.COM equivalent provides:

- **Prompt display**: Shows the current drive and directory (e.g., `A:¥>`)
  configurable via the PROMPT environment variable using MS-DOS meta-characters
  ($P = current path, $G = '>', $D = date, $T = time, etc.)
- **Line editing**: Character input with backspace, cursor left/right, Home, End,
  Insert (toggle overwrite), Delete
- **Command parsing**: Splits input into command name and argument string; handles
  I/O redirection (>, >>, <) and pipes (|)
- **Path search**: For external commands, searches the current directory first,
  then each directory in the PATH environment variable, trying .COM then .EXE extensions
- **Batch file execution**: Executes .BAT files line by line with variable substitution
  (%0-%9, %VARIABLE%)
- **Built-in commands**: CD, DIR, and other commands that operate on shell state directly
  (changing the current directory cannot be an external command because it must modify the
  shell's own CDS entry)

### 5.3 Command History

The shell maintains a ring buffer of the last 100 commands:

- **Up arrow**: Recall previous command (moving backward through history)
- **Down arrow**: Recall next command (moving forward through history)
- **Enter**: Execute current line and append to history (if non-empty and different from
  the last entry)
- History is in-memory only, not persisted across emulator sessions
- Duplicate consecutive commands are not stored

### 5.4 Built-In Shell Commands

These commands are part of the shell itself (not external .COM files on Z:) because they
modify shell-internal state:

- **CD (CHDIR)**: Change current directory
- **SET**: Display or modify environment variables
- **ECHO**: Display text or toggle echo state
- **REM**: Comment (no operation)
- **CLS**: Clear screen (via INT 18h AH=16h)
- **VER**: Display OS version

## 6. Commands

### 6.1 Command Trait

Each external command is implemented as a separate source file in `crates/os/src/commands/`.
All commands implement the `Command` trait:

```rust
pub trait Command {
    /// The primary command name (e.g., "DIR").
    fn name(&self) -> &'static str;

    /// Alternative names (e.g., &["ERASE"] for the DEL command).
    fn aliases(&self) -> &'static [&'static str] { &[] }

    /// Execute the command with the given argument string.
    /// Returns the process exit code (0 = success).
    fn execute(
        &self,
        args: &str,
        os: &mut OsState,
        disk: &mut dyn DiskIo,
        console: &mut dyn ConsoleIo,
    ) -> u8;
}
```

Commands use the `OsState` for file operations and the `ConsoleIo` trait for display and
keyboard input. They do not access CPU registers or emulated memory directly.

### 6.2 Command Reference

All commands are compatible with MS-DOS 6.20 command-line options.

| Command       | Aliases | File         | Description                           |
|---------------|---------|--------------|---------------------------------------|
| DIR           |         | dir.rs       | List files and directories            |
| COPY          |         | copy.rs      | Copy one or more files                |
| XCOPY         |         | xcopy.rs     | Copy files with directory trees       |
| DEL           | ERASE   | del.rs       | Delete one or more files              |
| MD            | MKDIR   | md.rs        | Create a directory                    |
| RD            | RMDIR   | rd.rs        | Remove a directory                    |
| TYPE          |         | type_cmd.rs  | Display contents of a text file       |
| MORE          |         | more.rs      | Paginated text file display           |
| DATE          |         | date.rs      | Show or change the current date       |
| TIME          |         | time.rs      | Show or change the current time       |
| FORMAT        |         | format.rs    | Format a floppy or hard disk          |
| DISKCOPY      |         | diskcopy.rs  | Copy entire floppy disk contents      |

#### DIR

Display directory listing. Supports: `/P` (pause per page), `/W` (wide format),
`/S` (include subdirectories), `/B` (bare format), `/O` (sort order: N/S/D/E),
`/A` (attribute filter: H/S/R/D/A). Wildcards: `*` and `?`.

#### COPY

Copy files. Supports: `/V` (verify), `/Y` (suppress overwrite prompt),
`/-Y` (prompt on overwrite), `/A` (ASCII text), `/B` (binary).
Supports concatenation with `+`.

#### XCOPY

Extended copy with directory trees. Supports: `/S` (copy subdirectories),
`/E` (include empty subdirectories), `/P` (prompt per file), `/V` (verify),
`/D:date` (copy files modified on or after date), `/Y` (suppress prompts).

#### DEL (ERASE)

Delete files. Supports: `/P` (prompt per file). Wildcards supported.
Prompts for confirmation when deleting `*.*`.

#### FORMAT

Format a disk. For floppy disks: writes FAT12 with boot sector, FAT tables, and root
directory. For hard disks: writes FAT16 with appropriate cluster size. Supports: `/Q`
(quick format - clear FAT and root directory only), `/U` (unconditional format),
`/S` (copy system files - not applicable for NEETAN OS), `/V:label` (volume label).

#### DISKCOPY

Copy the entire contents of one floppy disk to another. Reads source disk into memory,
prompts for target disk, writes all tracks. Supports: `/V` (verify after copy).

## 7. Configuration

### 7.1 CONFIG.SYS

If CONFIG.SYS exists on the boot drive (or the first mounted drive with a FAT filesystem),
NEETAN OS parses the following directives:

| Directive     | Default | Description                         |
|---------------|---------|-------------------------------------|
| FILES=n       | 20      | Maximum number of open file handles |
| BUFFERS=n     | 15      | Number of DOS disk buffers          |
| LASTDRIVE=x   | Z       | Last valid drive letter             |
| COUNTRY=nnn   | 081     | Country code (081 = Japan)          |
| BREAK=ON\|OFF | OFF     | Extended Ctrl-Break checking        |
| SHELL=path    | (none)  | Custom command interpreter path     |
| DEVICE=path   | (none)  | Load device driver (see below)      |

#### DEVICE= Handling

Most DEVICE= lines are silently ignored since NEETAN OS provides its own device
abstractions. The following drivers are recognized and handled specially:

- **NECCD.SYS / NECCDD.SYS**: CD-ROM device driver. NEETAN OS activates its built-in
  MSCDEX support instead of loading the driver binary. The `/D:name` parameter sets the
  device name for MSCDEX.

All other DEVICE= and DEVICEHIGH= lines are ignored with no error.

### 7.2 AUTOEXEC.BAT

If AUTOEXEC.BAT exists on the boot drive, its lines are executed sequentially as shell
commands after CONFIG.SYS processing. NEETAN OS supports:

- Simple commands (one per line)
- ECHO ON/OFF
- PAUSE
- REM (comments)
- SET (environment variables)
- PATH (sets PATH environment variable)
- PROMPT (sets PROMPT variable)
- GOTO and labels (:label) for flow control
- IF EXIST / IF NOT EXIST / IF ERRORLEVEL
- CALL (execute another batch file and return)
- %0-%9 parameter substitution
- %VARIABLE% environment variable expansion

Lines referencing unknown external programs (e.g., MOUSE.COM, SMARTDRV.EXE) produce a
"Bad command or file name" message and continue to the next line.

## 8. Compatibility

### 8.1 Version Reporting

- INT 21h AH=30h: Returns AL=6 (major), AH=20 (minor) for MS-DOS 6.20,
  BH=OEM serial number (NEC; standard values: IBM=00h, Microsoft=FFh;
  TODO: verify NEC's OEM ID from a real DOS 6.20 dump), BL=00h. The
  NEC-specific product number is more precisely identified via INT DCh
  CL=12h which reads 0060:0020h.
- INT 21h AH=33h/AL=06h: Returns BL=6 (major), BH=20 (minor) -- true DOS version 6.20.
  Note: BL holds the major version and BH holds the minor, which is the reverse of the
  AH=30h convention (where AL=major, AH=minor).
- INT 2Fh AX=1600h (Windows detection): Returns AL=02h on NEC MS-DOS 6.20 (the multiplex
  handler does not process this subfunction, so AL is modified by the default chain to 02h
  rather than remaining 00h). Valid "no Windows" responses are AL=00h, 01h, 02h, or 80h.

### 8.2 Country and Character Set

- Country code: 081 (Japan)
- Date format: YY/MM/DD
- Time format: 24-hour (HH:MM:SS)
- Currency symbol: 0x5C (displayed as yen sign on PC-98's character ROM)
- Thousands separator: comma (0x2C)
- Decimal separator: period (0x2E)
- DBCS lead byte table (Shift-JIS): 0x81-0x9F, 0xE0-0xFC
  The table lives in IO.SYS memory at a dynamic address (not at a fixed conventional
  memory location). Programs must call INT 21h AH=63h to obtain the pointer (DS:SI).
  Table format: pairs of (start, end) bytes terminated by (00h, 00h):
  81h, 9Fh, E0h, FCh, 00h, 00h.
  Detection: programs set DS:SI to 0000:0000 before calling; if DS:SI change, DBCS is active.
  AH=65h/AL=07h returns the same data prefixed by an info ID byte (07h) and length word.
- List separator: comma (0x2C)

### 8.3 Standard File Handles

Five standard handles are always open:

| Handle | Device | Description        |
|--------|--------|--------------------|
| 0      | CON    | Standard input     |
| 1      | CON    | Standard output    |
| 2      | CON    | Standard error     |
| 3      | AUX    | Auxiliary (serial) |
| 4      | PRN    | Printer            |

Handles 3 (AUX) and 4 (PRN) are stubs that accept writes silently and return EOF on reads.

### 8.4 TSR Support

Terminate and Stay Resident (INT 21h/31h) is supported. Resident programs can hook
interrupt vectors to intercept BIOS or DOS calls. Since the IVT lives in emulated RAM,
a TSR that replaces an interrupt vector causes the CPU to dispatch through the TSR's
handler in emulated memory first. If the TSR chains to the original vector, it eventually
reaches the ROM stub and triggers HLE dispatch.

## 9. crates/os Crate Design

### 9.1 Dependencies

```
crates/os/Cargo.toml:
  [dependencies]
  common = { path = "../common" }
```

The OS crate depends only on `common`. It does not depend on `device` or `machine`.
All hardware access goes through traits that the machine crate implements.

### 9.2 Module Structure

```
crates/os/src/
    lib.rs                 NeetanOs struct, top-level dispatch, boot sequence
    dos.rs                 INT 21h function dispatcher (AH routing)
    interrupt/
        int20.rs           INT 20h: Program Terminate
        int24.rs           INT 24h: Critical Error Handler (default)
        int25.rs           INT 25h: Absolute Disk Read
        int26.rs           INT 26h: Absolute Disk Write
        int28.rs           INT 28h: Idle
        int29.rs           INT 29h: Fast Console Output
        int2a.rs           INT 2Ah: Network / Critical Section
        int2f.rs           INT 2Fh: Multiplex (MSCDEX, XMS stubs, DOSKEY)
        int33.rs           INT 33h: Mouse Driver
        intdc.rs           INT DCh: NEC DOS Extension (IO.SYS replacement)
    memory.rs              MCB chain management (allocate, free, resize)
    process.rs             PSP creation, EXEC, terminate, process stack
    filesystem/
        mod.rs             Drive trait, DiskIo trait, drive mapping, error types
        fat.rs             FAT12/FAT16 read/write implementation
        fat_dir.rs         Directory entry parsing, creation, 8.3 name handling
        fat_bpb.rs         BIOS Parameter Block parsing and validation
        fat_partition.rs   PC-98 HDD partition table parsing
        virtual_drive.rs   Z: drive implementation
    console.rs             Console I/O trait and text VRAM interaction
    console_esc.rs         Native ESC sequence state machine and processing
    shell/
        mod.rs             Main shell loop, command parsing, I/O redirection
        history.rs         100-entry ring buffer with up/down navigation
        batch.rs           Batch file (.BAT) interpreter
        builtins.rs        Built-in shell commands (CD, SET, ECHO, CLS, VER, REM)
    commands/
        mod.rs             Command trait definition and command registry
        dir.rs             DIR
        copy.rs            COPY
        xcopy.rs           XCOPY
        del.rs             DEL / ERASE
        md.rs              MD / MKDIR
        rd.rs              RD / RMDIR
        type_cmd.rs        TYPE
        more.rs            MORE
        date.rs            DATE
        time.rs            TIME
        format.rs          FORMAT
        diskcopy.rs        DISKCOPY
    config.rs              CONFIG.SYS and AUTOEXEC.BAT parsing
    country.rs             Country info, DBCS lead byte table, date/time formats
    tables.rs              DOS internal data structures (SYSVARS, SFT, CDS, DPB layout)
    ioctl.rs               IOCTL (INT 21h/44h) dispatch
    cdrom.rs               MSCDEX state and CD-ROM device driver request handling
    state.rs               NeetanOsState for emulator save/load state
```

### 9.3 Key Types

```rust
/// Top-level OS instance, held as Option<NeetanOs> on Pc9801Bus.
pub struct NeetanOs {
    drives: [Option<Box<dyn Drive>>; 26],   // A-Z drive mapping
    current_drive: u8,                      // Default drive index (0=A)
    open_files: Vec<OpenFile>,              // System File Table entries
    process_stack: Vec<ProcessContext>,     // For nested EXEC
    dta_segment: u16,                       // Current DTA address
    dta_offset: u16,
    indos_flag: u8,                         // InDOS counter
    error_info: ExtendedError,              // Last error (INT 21h/59h)
    memory_strategy: u8,                    // Memory allocation strategy
    shell: Shell,                           // Command interpreter state
    commands: Vec<Box<dyn Command>>,        // Registered external commands
    config: DosConfig,                      // FILES=, BUFFERS=, etc.
    country: CountryInfo,                   // Country/codepage settings
    ctrl_break_check: bool,                 // Extended Ctrl-C checking
    switch_char: u8,                        // Switch character (/)
    verify_flag: bool,                      // VERIFY ON/OFF
    version: (u8, u8),                      // (6, 20)
    cdrom: Option<MscdexState>,             // MSCDEX state if CD-ROM present
}
```

### 9.4 Dispatch Interface

The machine crate calls into the OS through a single dispatch method:

```rust
impl NeetanOs {
    /// Handle a DOS/OS interrupt.
    /// vector: interrupt number (0x20-0x2F, 0x33)
    /// Returns true if handled, false if the vector should fall through.
    pub fn dispatch(
        &mut self,
        vector: u8,
        cpu: &mut dyn CpuAccess,
        memory: &mut dyn MemoryAccess,
        disk: &mut dyn DiskIo,
        console: &mut dyn ConsoleIo,
    ) -> bool;
}
```

The trait objects (`CpuAccess`, `MemoryAccess`, `DiskIo`, `ConsoleIo`) are defined in the
OS crate and implemented by the machine crate. This keeps the dependency graph clean:

```
common <-- os (defines traits, implements DOS logic)
common <-- device (hardware emulation)
common, device, os <-- machine (bridges traits to hardware)
```

### 9.5 Trait Definitions

```rust
/// CPU register access for the OS.
pub trait CpuAccess {
    fn ax(&self) -> u16;
    fn set_ax(&mut self, v: u16);
    fn bx(&self) -> u16;
    fn set_bx(&mut self, v: u16);
    fn cx(&self) -> u16;
    fn set_cx(&mut self, v: u16);
    fn dx(&self) -> u16;
    fn set_dx(&mut self, v: u16);
    fn si(&self) -> u16;
    fn set_si(&mut self, v: u16);
    fn di(&self) -> u16;
    fn set_di(&mut self, v: u16);
    fn ds(&self) -> u16;
    fn set_ds(&mut self, v: u16);
    fn es(&self) -> u16;
    fn set_es(&mut self, v: u16);
    fn ss(&self) -> u16;
    fn sp(&self) -> u16;
    fn set_sp(&mut self, v: u16);
    fn cs(&self) -> u16;
    // Flags manipulation for IRET frame
    fn set_carry(&mut self, carry: bool);
}

/// Emulated memory access for the OS.
pub trait MemoryAccess {
    fn read_byte(&self, address: u32) -> u8;
    fn write_byte(&mut self, address: u32, value: u8);
    fn read_word(&self, address: u32) -> u16;
    fn write_word(&mut self, address: u32, value: u16);
    /// Bulk read from emulated RAM into a host buffer.
    fn read_block(&self, address: u32, buf: &mut [u8]);
    /// Bulk write from a host buffer into emulated RAM.
    fn write_block(&mut self, address: u32, data: &[u8]);
}

/// Console I/O for commands and the shell.
pub trait ConsoleIo {
    /// Write a character to the console at the current cursor position.
    fn write_char(&mut self, ch: u8);
    /// Write a string to the console.
    fn write_str(&mut self, s: &[u8]);
    /// Read a character from the keyboard buffer (blocking).
    fn read_char(&mut self) -> u8;
    /// Check if a character is available in the keyboard buffer.
    fn char_available(&self) -> bool;
    /// Read a scan code + character pair (for special keys like arrows).
    fn read_key(&mut self) -> (u8, u8); // (scan_code, ascii)
    /// Get current cursor position.
    fn cursor_position(&self) -> (u8, u8); // (row, col)
    /// Set cursor position.
    fn set_cursor_position(&mut self, row: u8, col: u8);
    /// Scroll the screen up by one line.
    fn scroll_up(&mut self);
    /// Clear the screen.
    fn clear_screen(&mut self);
    /// Get the screen dimensions.
    fn screen_size(&self) -> (u8, u8); // (rows, cols)
}
```

## 10. Implementation Roadmap

Ten phases, each building on the previous. Every phase is independently testable against
the `crates/os/tests/dos620/` integration test suite.

### 10.1 Trampoline Wiring and Crate Skeleton

- Add DOS interrupt stubs (INT 20h-2Fh, 33h, DCh) to `utils/bios/bios.asm` using existing `hle_stub` macro
- Create `NeetanOs` struct with `dispatch()` and `boot()` in `crates/os/src/lib.rs`
- Define traits: `CpuAccess`, `MemoryAccess`, `DiskIo`, `ConsoleIo`
- Add `Option<NeetanOs>` to `Pc9801Bus`; wire `execute_bios_hle()` to forward DOS vectors
- Create module file skeleton per section 9.2 (empty files with declarations)

**Tests**: None (HLE test harness not yet built). Verify existing real-DOS boot still works.

**Milestone**: DOS interrupts reach Rust `dispatch()`. Unhandled calls fall through harmlessly.

### 10.2 Memory Layout and Boot Data Structures

- `boot()` writes SYSVARS / List of Lists at 0x0600 with all fields from section 2.2
- Device header chain: NUL (in SYSVARS), CON, CLOCK, $AID#NEC, MS$KANJI
- SFT header + 5 standard entries (stdin/stdout/stderr/aux/prn mapped to CON)
- IO.SYS work area at segment 0060h (product number, DA/UA tables, cursor, display, scroll)
- InDOS flag, critical error flag, FCB-SFT header, disk buffer header + one buffer

**Tests**: `memory_layout`, `sysvars`, `iosys_workarea`

**Milestone**: `boot()` populates valid DOS data structures in RAM. INT 21h/52h returns SYSVARS.

### 10.3 MCB Chain, PSP, and Environment Block

- MCB chain management: `allocate()`, `free()`, `resize()` operating on MCBs in emulated RAM
- Initial chain: sentinel MCB, environment block, COMMAND.COM block, free Z-block
- PSP creation: 256-byte structure with handle table, INT 20h instruction, far call stub, saved vectors
- Environment block: COMSPEC, PATH, PROMPT strings; no WORD count + pathname for COMMAND.COM (section 2.6)
- COMMAND.COM code stub at PSP:0100h (MOV AH,FFh / INT 21h / JMP loop)

**Tests**: `mcb_chain`, `psp`, `environment`

**Milestone**: Process infrastructure in place. COMMAND.COM has valid PSP and environment.

### 10.4 Drive System and Disk Parameter Blocks

- Drive letter assignment: enumerate floppies, read PC-98 HDD partition tables, assign letters per section 3.1
- CDS array in RAM: current path and flags (0x4000 for physical) per mounted drive
- DPB chain in RAM: geometry from BPB, FAT info, media descriptor, device header pointers
- DA/UA mapping tables at 0060:006Ch and 0060:2C86h
- Update SYSVARS: block device count, boot drive, max bytes per sector

**Tests**: `drives`, `data_structures` (CDS path/flags, DPB fields)

**Milestone**: Drive letters assigned, DPB/CDS populated. Programs can query drive geometry.

### 10.5 Core INT 21h Dispatch (Non-File, Non-Console)

- INT 21h AH-based function dispatcher in `dos.rs`
- Country info and DBCS lead byte table (Japan 081, Shift-JIS ranges)
- Functions: AH=0Eh, 19h, 1Ah, 25h, 2Fh, 30h, 33h, 34h, 35h, 37h, 38h, 3Bh, 47h, 48h, 49h, 4Ah, 4Dh, 50h, 51h/62h, 52h, 58h, 63h, 65h
- INT 20h terminate stub, INT 2Ah critical section stubs

**Tests**: `syscalls_int21h`, `compatibility`, `data_structures` (InDOS, DBCS, SFT)

**Milestone**: Programs get "MS-DOS 6.20" from version check. Memory management works.

### 10.6 INT DCh, INT 2Fh, and INT 28h/29h

- INT DCh: CL=00h-08h no-ops, CL=12h system ID, CL=13h DA/UA buffer, CL=15h revision, CL=80h partition info, CL=81h extended memory
- INT 2Fh: AX=1600h Windows check, AX=4300h XMS, AX=4800h DOSKEY, AX=4A01h HMA
- INT 28h idle (IRET), INT 29h fast console output stub

**Tests**: `syscalls_intdch`, `syscalls_int2fh`

**Milestone**: NEC extensions and multiplex interrupt functional.

### 10.7 Console I/O and ESC Sequence Processing

- Console output via text VRAM writes with cursor tracking
- ESC sequence state machine for PC-98 native sequences (cursor, erase, scroll, attributes)
- INT 21h: AH=02h, 06h, 07h, 08h, 09h, 0Ah, 0Ch
- INT 29h full implementation, INT DCh CL=10h console subfunctions
- Update IO.SYS work area cursor position on every move

**Tests**: `syscalls_int21h_console`

**Milestone**: Text display works. Last prerequisite before shell can show a prompt.

### 10.8 FAT Filesystem and File I/O

- FAT12/FAT16 driver: FAT table read/write, cluster chain traversal, allocation/deallocation
- Directory operations: 8.3 name parsing, entry search with wildcards, create/delete entries
- Virtual Z: drive: read-only filesystem listing built-in commands as .COM files
- SFT entry management, handle-to-SFT mapping via PSP JFT
- SJIS-aware path parsing (DBCS 0x5C handling)
- INT 21h: AH=0Dh, 1Ch, 29h, 3Ch, 3Dh, 3Eh, 3Fh, 40h, 41h, 42h, 43h, 44h, 45h, 4Eh, 4Fh, 56h, 57h, 5Dh
- INT 25h/26h absolute disk read/write, IOCTL dispatch

**Tests**: `syscalls_int21h_file_io`

**Milestone**: Full file I/O operational. Programs can open, read, write, and search files on FAT volumes.

### 10.9 Process Management (EXEC and Terminate)

- .COM loading: find largest free MCB, allocate, create child PSP, load at PSP:0100h
- .EXE (MZ) loading: parse header, calculate size, allocate, create PSP, load, apply relocations
- Z: drive shortcut: execute Rust `Command::execute()` directly for built-in commands
- Process stack: push/pop ProcessContext on nested EXEC, save/restore INT 22h/23h/24h
- Termination: INT 20h, INT 21h/4Ch, INT 21h/31h (TSR), INT 27h
- Teardown: close JFT handles, free MCBs (or resize for TSR), pop parent context, set return code

**Tests**: New integration tests for .COM/.EXE loading and termination

**Milestone**: Programs can be loaded and executed. TSR programs stay resident.

### 10.10 Shell, Commands, and Configuration

- Shell main loop: display prompt, read line, parse command, dispatch, I/O redirection (>, >>, <), pipes
- Command history: 100-entry ring buffer, up/down arrow navigation
- Built-in commands: CD, SET, ECHO, REM, CLS, VER
- Batch interpreter: .BAT processing, GOTO/labels, IF, CALL, variable substitution (%0-%9, %VAR%)
- External commands via Command trait: DIR, COPY, DEL, MD, RD, TYPE, MORE, DATE, TIME, FORMAT, DISKCOPY
- CONFIG.SYS parser: FILES=, BUFFERS=, LASTDRIVE=, COUNTRY=, BREAK=, SHELL=, DEVICE=
- AUTOEXEC.BAT execution
- MSCDEX: INT 2Fh AH=15h subfunctions (install check, drive letters, version, device request)
- Bootstrap completion: boot() -> parse CONFIG.SYS -> create COMMAND.COM -> run AUTOEXEC.BAT -> prompt

**Tests**: `config`, all 142 tests pass against HLE OS

**Milestone**: Boots to a functional command prompt. Full DOS replacement operational.

---

## SYSCALLS

Observed software interrupts from booting and using four PC-98 games (Yuno, Doom, A-Train 4, Edge).

The interrupts below show ALL syscalls observed and may include syscalls, that are called by DOS or MSCDEX themselve.

### DOS Interrupts (INT 20h-2Fh)

#### INT 21h -- DOS Function Calls

- AH=02h: Display character (DL = character)
- AH=06h: Direct console I/O
- AH=09h: Display string (DS:DX = '$'-terminated string)
- AH=0Ah: Buffered keyboard input
- AH=0Ch: Flush input buffer, then invoke input function
- AH=0Dh: Disk reset (flush all file buffers)
- AH=0Eh: Select default drive
- AH=19h: Get current default drive
- AH=1Ah: Set Disk Transfer Area (DTA) address
- AH=1Ch: Get allocation information for specific drive
- AH=25h: Set interrupt vector
- AH=29h: Parse filename into FCB
- AH=2Fh: Get DTA address
- AH=30h: Get DOS version number
- AH=31h: Terminate and Stay Resident (TSR)
- AH=33h: Get/set Ctrl-Break check state
- AH=34h: Get address of InDOS flag
- AH=35h: Get interrupt vector
- AH=37h: Get/set switch character (undocumented)
- AH=38h: Get/set country-dependent information
- AH=3Bh: Change current directory (CHDIR)
- AH=3Ch: Create file
- AH=3Dh: Open file
- AH=3Eh: Close file handle
- AH=3Fh: Read from file or device
- AH=40h: Write to file or device
- AH=42h: Move file pointer (LSEEK)
- AH=41h: Delete file (DS:DX = ASCIZ filename)
- AH=43h: Get/set file attributes
- AH=44h: IOCTL (I/O control for devices)
- AH=45h: Duplicate file handle (DUP)
- AH=47h: Get current directory
- AH=48h: Allocate memory block
- AH=49h: Free memory block
- AH=4Ah: Resize memory block (SETBLOCK)
- AH=4Bh: Execute program (EXEC)
- AH=4Ch: Terminate process with return code
- AH=4Dh: Get return code of child process (WAIT)
- AH=4Eh: Find first matching file (FINDFIRST)
- AH=4Fh: Find next matching file (FINDNEXT)
- AH=50h: Set current PSP address (undocumented)
- AH=51h: Get current PSP address (undocumented)
- AH=52h: Get List of Lists / SYSVARS (undocumented)
- AH=57h: Get/set file date and time
- AH=58h: Get/set memory allocation strategy
- AH=5Dh: Set extended error information (undocumented)
- AH=62h: Get PSP address
- AH=63h: Get lead byte table (DBCS double-byte support)
- AH=65h: Get extended country information

#### INT 24h -- Critical Error Handler

- No AH function dispatch. Invoked by DOS on critical errors; AH value is context-dependent, not a function selector.

#### INT 28h -- DOS Idle Interrupt

- No AH function dispatch. Called by DOS when waiting for input; signals TSRs that DOS is idle.

#### INT 29h -- DOS Fast Console Output

- No AH function dispatch. Outputs character in AL to console; AH is not a function selector.

#### INT 2Ah -- DOS Network / Critical Section

- AH=80h: Begin critical section
- AH=81h: End critical section
- AH=82h: Check network installation
- AH=84h: Get network interval count

#### INT 2Fh -- DOS Multiplex Interrupt

- AH=11h: Network redirector (IFSFUNC)
- AH=12h: DOS internal services (undocumented)
- AH=15h: MSCDEX (CD-ROM extensions)
- AH=16h: Windows enhanced mode notification
- AH=19h: DOS internal -- shell services
- AH=43h: XMS (Extended Memory Specification) driver
- AH=48h: DOSKEY interface
- AH=4Ah: HMA (High Memory Area) management
- AH=4Dh: GRAFTABL interface
- AH=4Fh: Installable INT 21h hook
- AH=55h: COMMAND.COM interface
- AH=AEh: Installable command check (COMMAND.COM)
- AH=B7h: APPEND interface

### NEC DOS Extension (INT DCh)

INT DCh is a NEC-only DOS extension interrupt provided by IO.SYS. It is dispatched by
the CL register with subfunctions in AX. It has no IBM PC equivalent. NEETAN OS must
provide this handler since it replaces IO.SYS.

The trampoline mechanism requires a new stub: `int_dch_handler: hle_stub 0xDC`

#### Observed INT DCh Calls

(To be expanded as more games are traced)

#### INT DCh Functions to Implement

| CL     | Function                | Priority | Description                                                                    |
|--------|-------------------------|----------|--------------------------------------------------------------------------------|
| 00-08h | No-op                   | Required | Must return without modifying registers                                        |
| 09h    | SCSI device query       | Low      | AX=0000h: device type list (8B to DS:DX); AX=0010h/0011h: MO eject lock/unlock |
| 0Ah    | RS-232C initialization  | Low      | Serial port setup                                                              |
| 0Ch    | Function key read       | Medium   | Read programmable function key definitions                                     |
| 0Dh    | Function key write      | Medium   | Write programmable function key definitions                                    |
| 0Eh    | RS-232C subfunctions    | Low      | Serial port configuration                                                      |
| 0Fh    | Ctrl+Fn softkey control | Medium   | Enable/disable Ctrl+Fn key capture                                             |
| 10h    | Console display         | High     | 15 subfunctions for screen output and cursor                                   |
| 11h    | Printer control         | Low      | Kanji/ANK ratio, vertical writing mode                                         |
| 12h    | System identification   | High     | AX=product number (0060:0020h), DX=machine info                                |
| 13h    | Drive DA/UA mapping     | High     | Fills 96-byte buffer at DS:DX (see below)                                      |
| 14h    | Display mode switching  | Medium   | Extended attribute and text mode control                                       |
| 15h    | Internal revision       | Medium   | Returns IO.SYS revision from 0060:0022h                                        |
| 80h    | Disk/partition info     | Medium   | Retrieves disk drive/partition information                                     |
| 81h    | Extended memory query   | Medium   | Returns extended memory size from 0060:0031h                                   |
| 82h    | Extended memory range   | Low      | Returns available extended memory range                                        |
| E0-FDh | FEP/IME integration     | Low      | Japanese input method extension functions                                      |

INT DCh CL=12h detail: Programs detect support by calling with AX=0000h; if AX remains
0000h on return, the function is not supported by this DOS version. For MS-DOS 5.0+,
product numbers are in the 0100h+ range (e.g. 0102h for 5.0A). DX returns machine type
information (0003h = standard normal-mode PC-98).

INT DCh CL=13h buffer layout (96 bytes at DS:DX):

| Offset   | Size     | Content                                                 |
|----------|----------|---------------------------------------------------------|
| +00h-0Fh | 16 BYTES | A:-P: DA/UA bytes (legacy, one byte per drive)          |
| +10h-19h | 10 BYTES | Reserved (00h)                                          |
| +1Ah-4Dh | 52 BYTES | A:-Z: attribute+DA/UA pairs (2 bytes per drive)         |
| +4Eh     | BYTE     | FD logical drive duplicate flag (copy of 0060:0038h)    |
| +4Fh     | BYTE     | FD logical drive duplicate flag (copy of 0060:013Bh)    |
| +50h     | BYTE     | Last accessed drive number, A:=00h (copy of 0060:0136h) |
| +51h-5Fh | 15 BYTES | Reserved (00h)                                          |

In the extended section (+1Ah-4Dh), odd offsets contain DA/UA values, even offsets
contain attribute bytes (bit 7=dual-drive, bit 1=32-bit sectors, bit 0=MO device).

INT DCh CL=10h subfunctions (dispatched by AH):
- AH=00h: Single character output
- AH=01h: String display
- AH=02h: Set attribute
- AH=03h: Cursor positioning
- AH=04h-09h: Cursor movement (up/down/left/right/home/end)
- AH=0Ah: Erase in display
- AH=0Bh: Erase in line
- AH=0Ch: Insert lines (scroll down)
- AH=0Dh: Delete lines (scroll up)
- AH=0Eh: Kanji/graph mode switching

### Other Interrupts (INT 30h+)

#### INT 33h -- Mouse Driver

- AH=00h: Mouse reset and status
- AH=03h: Get cursor position and button status
- AH=07h: Set horizontal min/max range
- AH=08h: Set vertical min/max range
- AH=0Bh: Read motion counters

#### Application-Installed Vectors (not OS-provided)

The following interrupt vectors appear in game traces but are installed by applications
themselves at runtime. NEETAN OS does not need to provide handlers for these -- it only
needs to leave the IVT slots available for programs to hook.

- **INT 3Fh (Overlay Manager)**: Installed by application runtimes (Borland VROOMM, Watcom,
  etc.) for loading overlay segments. The compiled executable sets up its own INT 3Fh handler
  at startup.
- **INT 60h (Application/TSR Private)**: INT 60h-67h are reserved for user software.
  Applications and TSRs install their own handlers on these vectors.

#### Hardware BIOS Vectors (not OS-provided)

The following interrupt vectors appear in game traces but are hardware BIOS functions
provided by adapter ROMs or the system BIOS. They are handled by the emulator's bus HLE
layer, not by the OS.

- **INT D2h**: NEC SCSI BIOS (SCSI adapter ROM)
- **INT D3h**: PC-98 Sound BIOS (sound board ROM)
- **INT D4h**: PC-98 Sound BIOS Extended (sound board ROM)

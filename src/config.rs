use std::path::{Path, PathBuf};

use common::{Context, MachineModel, StringError, bail, warn};

use crate::keymap::{self, KeyMap};

fn next_value(flag: &str, args: &mut impl Iterator<Item = String>) -> crate::Result<String> {
    match args.next() {
        Some(val) => Ok(val),
        None => bail!("missing value for {flag}"),
    }
}

fn parse_on_off(val: &str, flag: &str) -> crate::Result<bool> {
    match val {
        "on" => Ok(true),
        "off" => Ok(false),
        _ => bail!("invalid value '{val}' for {flag}, expected on or off"),
    }
}

fn print_help() {
    println!(
        "\
{} — PC-98 emulator

Usage: neetan [OPTIONS]
       neetan <COMMAND>

Commands:
  create-fdd <PATH>             Create an empty floppy disk image (D88)
  create-hdd <PATH>             Create an empty hard disk image (HDI)
  convert-hdd <INPUT> <OUTPUT>  Convert HDD image between SASI and IDE

Options:
  -c, --config <PATH>         Load configuration from file
      --machine <TYPE>        Machine type: PC9801VM, PC9801VX, PC9801RA, PC9821AS, PC9821AP
      --fdd1 <PATH>           Floppy disk image for drive 1 (repeatable)
      --fdd2 <PATH>           Floppy disk image for drive 2 (repeatable)
      --hdd1 <PATH>           Hard disk image for drive 1 (SASI or IDE)
      --hdd2 <PATH>           Hard disk image for drive 2 (SASI or IDE)
      --cdrom <PATH>          CD-ROM disc image CUE file (repeatable, PC-9821 only)
      --audio-volume <FLOAT>  Audio volume 0.0-1.0
      --aspect-mode <MODE>    Display aspect mode: 4:3 or 1:1
      --window-mode <MODE>    Window mode: windowed or fullscreen
      --bios-rom <PATH>       Path to BIOS ROM file
      --font-rom <PATH>       Path to font ROM file
      --soundboard <TYPE>     Sound board type: none, 26k, 86, 86+26k, sb16, sb16+26k
      --adpcm-ram <on|off>    ADPCM RAM option for PC-9801-86 (default: on)
      --gdc-compatibility     Force 2.5 MHz GDC clock (200-line compatibility mode)
      --printer <PATH>        Output file for printer (must exist)
  -h, --help                  Print help
  -V, --version               Print version

Run 'neetan <COMMAND> --help' for more information on a command.",
        crate::GAME_NAME,
    );
}

fn print_create_fdd_help() {
    println!(
        "\
Create an empty floppy disk image in D88 format

Usage: neetan create-fdd <PATH> [OPTIONS]

Arguments:
  <PATH>  Output file path (must have .d88 extension)

Options:
      --type <TYPE>  Floppy type [default: 2hd]
  -h, --help         Print help

Floppy types:
  2hd    1232 KB  (77 cyl, 2 heads, 8 spt, 1024 B/sector)
  2dd     640 KB  (80 cyl, 2 heads, 16 spt, 256 B/sector)"
    );
}

fn print_create_hdd_help() {
    println!(
        "\
Create an empty hard disk image in HDI format

Usage: neetan create-hdd <PATH> [OPTIONS]

Arguments:
  <PATH>  Output file path (must have .hdi extension)

Options:
      --type <TYPE>  HDD size (required)
  -h, --help         Print help

SASI types:
  sasi5      5 MB  (153 cyl, 4 heads, 33 spt, 256 B/sector)
  sasi10    10 MB  (310 cyl, 4 heads, 33 spt, 256 B/sector)
  sasi15    15 MB  (310 cyl, 6 heads, 33 spt, 256 B/sector)
  sasi20    20 MB  (310 cyl, 8 heads, 33 spt, 256 B/sector)
  sasi30    30 MB  (615 cyl, 6 heads, 33 spt, 256 B/sector)
  sasi40    40 MB  (615 cyl, 8 heads, 33 spt, 256 B/sector)

IDE types:
  ide40     40 MB  (977 cyl, 5 heads, 17 spt, 512 B/sector)
  ide80     80 MB  (977 cyl, 10 heads, 17 spt, 512 B/sector)
  ide120   120 MB  (977 cyl, 15 heads, 17 spt, 512 B/sector)
  ide200   200 MB  (977 cyl, 15 heads, 28 spt, 512 B/sector)
  ide500   500 MB  (1015 cyl, 16 heads, 63 spt, 512 B/sector)"
    );
}

fn print_convert_hdd_help() {
    println!(
        "\
Convert a hard disk image between SASI and IDE formats

Usage: neetan convert-hdd <INPUT> <OUTPUT>

Arguments:
  <INPUT>   Source HDD image (HDI, NHD, or THD)
  <OUTPUT>  Destination path (must have .hdi extension)

Options:
  -h, --help  Print help

The conversion direction is detected from the input image:
  256 B/sector (SASI) -> converts to IDE
  512 B/sector (IDE)  -> converts to SASI

The smallest compatible target geometry is chosen automatically.

SASI geometries:
  sasi5      5 MB  (153 cyl, 4 heads, 33 spt, 256 B/sector)
  sasi10    10 MB  (310 cyl, 4 heads, 33 spt, 256 B/sector)
  sasi15    15 MB  (310 cyl, 6 heads, 33 spt, 256 B/sector)
  sasi20    20 MB  (310 cyl, 8 heads, 33 spt, 256 B/sector)
  sasi30    30 MB  (615 cyl, 6 heads, 33 spt, 256 B/sector)
  sasi40    40 MB  (615 cyl, 8 heads, 33 spt, 256 B/sector)

IDE geometries:
  ide40     40 MB  (977 cyl, 5 heads, 17 spt, 512 B/sector)
  ide80     80 MB  (977 cyl, 10 heads, 17 spt, 512 B/sector)
  ide120   120 MB  (977 cyl, 15 heads, 17 spt, 512 B/sector)
  ide200   200 MB  (977 cyl, 15 heads, 28 spt, 512 B/sector)
  ide500   500 MB  (1015 cyl, 16 heads, 63 spt, 512 B/sector)"
    );
}

fn print_version() {
    println!("neetan {}", crate::CARGO_PKG_VERSION);
}

pub enum Action {
    Run(Box<EmulatorConfig>),
    CreateFdd {
        path: PathBuf,
        fdd_type: FddType,
    },
    CreateHdd {
        path: PathBuf,
        hdd_type: HddSizeType,
    },
    ConvertHdd {
        input: PathBuf,
        output: PathBuf,
    },
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum FddType {
    Hd2,
    Dd2,
}

impl std::str::FromStr for FddType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "2hd" => Ok(Self::Hd2),
            "2dd" => Ok(Self::Dd2),
            _ => Err(format!("unknown floppy type '{s}', expected 2hd or 2dd")),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum HddSizeType {
    Mb5,
    Mb10,
    Mb15,
    Mb20,
    Mb30,
    Mb40,
    IdeMb40,
    IdeMb80,
    IdeMb120,
    IdeMb200,
    IdeMb500,
}

impl std::str::FromStr for HddSizeType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "sasi5" => Ok(Self::Mb5),
            "sasi10" => Ok(Self::Mb10),
            "sasi15" => Ok(Self::Mb15),
            "sasi20" => Ok(Self::Mb20),
            "sasi30" => Ok(Self::Mb30),
            "sasi40" => Ok(Self::Mb40),
            "ide40" => Ok(Self::IdeMb40),
            "ide80" => Ok(Self::IdeMb80),
            "ide120" => Ok(Self::IdeMb120),
            "ide200" => Ok(Self::IdeMb200),
            "ide500" => Ok(Self::IdeMb500),
            _ => Err(format!(
                "unknown HDD size '{s}', expected sasi5, sasi10, sasi15, sasi20, sasi30, sasi40, ide40, ide80, ide120, ide200, or ide500"
            )),
        }
    }
}

fn parse_create_fdd_args(args: &mut impl Iterator<Item = String>) -> crate::Result<Action> {
    let mut path: Option<PathBuf> = None;
    let mut fdd_type = FddType::Hd2;

    while let Some(arg) = args.next() {
        let (flag, inline_value) = match arg.split_once('=') {
            Some((f, v)) => (f.to_owned(), Some(v.to_owned())),
            None => (arg, None),
        };

        match flag.as_str() {
            "--help" | "-h" => {
                print_create_fdd_help();
                std::process::exit(0);
            }
            "--type" => {
                let val = if let Some(v) = inline_value {
                    v
                } else {
                    next_value("--type", args)?
                };
                fdd_type = val.parse::<FddType>().map_err(StringError)?;
            }
            other if !other.starts_with('-') && path.is_none() => {
                path = Some(PathBuf::from(other));
            }
            other => bail!("unknown argument: {other}"),
        }
    }

    let path = path.ok_or_else(|| StringError("missing required argument: <PATH>".into()))?;
    Ok(Action::CreateFdd { path, fdd_type })
}

fn parse_create_hdd_args(args: &mut impl Iterator<Item = String>) -> crate::Result<Action> {
    let mut path: Option<PathBuf> = None;
    let mut hdd_type: Option<HddSizeType> = None;

    while let Some(arg) = args.next() {
        let (flag, inline_value) = match arg.split_once('=') {
            Some((f, v)) => (f.to_owned(), Some(v.to_owned())),
            None => (arg, None),
        };

        match flag.as_str() {
            "--help" | "-h" => {
                print_create_hdd_help();
                std::process::exit(0);
            }
            "--type" => {
                let val = if let Some(v) = inline_value {
                    v
                } else {
                    next_value("--type", args)?
                };
                hdd_type = Some(val.parse::<HddSizeType>().map_err(StringError)?);
            }
            other if !other.starts_with('-') && path.is_none() => {
                path = Some(PathBuf::from(other));
            }
            other => bail!("unknown argument: {other}"),
        }
    }

    let path = path.ok_or_else(|| StringError("missing required argument: <PATH>".into()))?;
    let hdd_type =
        hdd_type.ok_or_else(|| StringError("missing required option: --type <TYPE>".into()))?;
    Ok(Action::CreateHdd { path, hdd_type })
}

fn parse_convert_hdd_args(args: &mut impl Iterator<Item = String>) -> crate::Result<Action> {
    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;

    for arg in args {
        match arg.as_str() {
            "--help" | "-h" => {
                print_convert_hdd_help();
                std::process::exit(0);
            }
            other if !other.starts_with('-') => {
                if input.is_none() {
                    input = Some(PathBuf::from(other));
                } else if output.is_none() {
                    output = Some(PathBuf::from(other));
                } else {
                    bail!("unexpected argument: {other}");
                }
            }
            other => bail!("unknown argument: {other}"),
        }
    }

    let input = input.ok_or_else(|| StringError("missing required argument: <INPUT>".into()))?;
    let output = output.ok_or_else(|| StringError("missing required argument: <OUTPUT>".into()))?;
    Ok(Action::ConvertHdd { input, output })
}

pub fn parse_args() -> crate::Result<Action> {
    let mut config = EmulatorConfig::default();
    let mut args = std::env::args().skip(1);

    while let Some(arg) = args.next() {
        if arg == "create-fdd" {
            return parse_create_fdd_args(&mut args);
        }
        if arg == "create-hdd" {
            return parse_create_hdd_args(&mut args);
        }
        if arg == "convert-hdd" {
            return parse_convert_hdd_args(&mut args);
        }
        let (flag, inline_value) = match arg.split_once('=') {
            Some((f, v)) => (f.to_owned(), Some(v.to_owned())),
            None => (arg, None),
        };

        let mut value = |flag: &str| -> crate::Result<String> {
            if let Some(v) = inline_value.clone() {
                Ok(v)
            } else {
                next_value(flag, &mut args)
            }
        };

        match flag.as_str() {
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            "--version" | "-V" => {
                print_version();
                std::process::exit(0);
            }
            "-c" | "--config" => {
                let path = value(&flag)?;
                config = parse_config_file(Path::new(&path))?;
            }
            "--machine" => {
                let val = value(&flag)?;
                config.machine = val.parse::<MachineModel>().map_err(StringError)?;
            }
            "--fdd1" => config.fdd1.push(PathBuf::from(value(&flag)?)),
            "--fdd2" => config.fdd2.push(PathBuf::from(value(&flag)?)),
            "--hdd1" => config.hdd1 = Some(PathBuf::from(value(&flag)?)),
            "--hdd2" => config.hdd2 = Some(PathBuf::from(value(&flag)?)),
            "--cdrom" => config.cdrom.push(PathBuf::from(value(&flag)?)),
            "--audio-volume" => {
                let val = value(&flag)?;
                config.audio_volume = val
                    .parse::<f32>()
                    .map_err(|e| StringError(format!("invalid audio volume '{val}': {e}")))?;
            }
            "--aspect-mode" => {
                let val = value(&flag)?;
                config.aspect_mode = val.parse::<AspectMode>().map_err(StringError)?;
            }
            "--window-mode" => {
                let val = value(&flag)?;
                config.window_mode = val.parse::<WindowMode>().map_err(StringError)?;
            }
            "--bios-rom" => config.bios_rom = Some(PathBuf::from(value(&flag)?)),
            "--font-rom" => config.font_rom = Some(PathBuf::from(value(&flag)?)),
            "--soundboard" => {
                let val = value(&flag)?;
                config.soundboard = val.parse::<SoundboardType>().map_err(StringError)?;
            }
            "--adpcm-ram" => {
                let val = value(&flag)?;
                config.adpcm_ram = parse_on_off(&val, &flag)?;
            }
            "--gdc-compatibility" => config.gdc_compatibility = true,
            "--printer" => config.printer = Some(PathBuf::from(value(&flag)?)),
            other => bail!("unknown argument: {other}"),
        }
    }

    validate_paths(&config)?;

    Ok(Action::Run(Box::new(config)))
}

fn validate_paths(config: &EmulatorConfig) -> crate::Result<()> {
    for path in &config.fdd1 {
        if !path.exists() {
            bail!("fdd1 image not found: {}", path.display());
        }
    }
    for path in &config.fdd2 {
        if !path.exists() {
            bail!("fdd2 image not found: {}", path.display());
        }
    }
    for path in &config.cdrom {
        if !path.exists() {
            bail!("cdrom image not found: {}", path.display());
        }
    }
    if let Some(ref path) = config.hdd1
        && !path.exists()
    {
        bail!("hdd1 image not found: {}", path.display());
    }
    if let Some(ref path) = config.hdd2
        && !path.exists()
    {
        bail!("hdd2 image not found: {}", path.display());
    }
    Ok(())
}

pub struct EmulatorConfig {
    pub machine: MachineModel,
    pub fdd1: Vec<PathBuf>,
    pub fdd2: Vec<PathBuf>,
    pub hdd1: Option<PathBuf>,
    pub hdd2: Option<PathBuf>,
    pub cdrom: Vec<PathBuf>,
    pub aspect_mode: AspectMode,
    pub window_mode: WindowMode,
    pub audio_volume: f32,
    pub bios_rom: Option<PathBuf>,
    pub font_rom: Option<PathBuf>,
    pub soundboard: SoundboardType,
    pub adpcm_ram: bool,
    pub gdc_compatibility: bool,
    pub printer: Option<PathBuf>,
    pub key_map: KeyMap,
}

impl Default for EmulatorConfig {
    fn default() -> Self {
        Self {
            machine: MachineModel::PC9801VX,
            fdd1: Vec::new(),
            fdd2: Vec::new(),
            hdd1: None,
            hdd2: None,
            cdrom: Vec::new(),
            aspect_mode: AspectMode::Aspect4By3,
            window_mode: WindowMode::Windowed,
            audio_volume: 1.0,
            bios_rom: None,
            font_rom: None,
            soundboard: SoundboardType::Sb86And26k,
            adpcm_ram: true,
            gdc_compatibility: false,
            printer: None,
            key_map: KeyMap::new(),
        }
    }
}

pub fn parse_config_file(path: &Path) -> crate::Result<EmulatorConfig> {
    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;

    let mut config = EmulatorConfig::default();

    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        let Some((key, val)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let val = val.trim();
        match key {
            "machine" => match val.parse::<MachineModel>() {
                Ok(mt) => config.machine = mt,
                Err(_) => warn!("Unknown machine type in config: {val}"),
            },
            "fdd1" => config.fdd1.push(PathBuf::from(val)),
            "fdd2" => config.fdd2.push(PathBuf::from(val)),
            "hdd1" => config.hdd1 = Some(PathBuf::from(val)),
            "hdd2" => config.hdd2 = Some(PathBuf::from(val)),
            "cdrom" => config.cdrom.push(PathBuf::from(val)),
            "aspect-mode" => match val.parse::<AspectMode>() {
                Ok(mode) => config.aspect_mode = mode,
                Err(_) => warn!("Unknown aspect mode in config: {val}"),
            },
            "window-mode" => match val.parse::<WindowMode>() {
                Ok(mode) => config.window_mode = mode,
                Err(_) => warn!("Unknown window mode in config: {val}"),
            },
            "audio-volume" => match val.parse::<f32>() {
                Ok(v) => config.audio_volume = v,
                Err(_) => warn!("Invalid audio-volume in config: {val}"),
            },
            "bios-rom" => config.bios_rom = Some(PathBuf::from(val)),
            "font-rom" => config.font_rom = Some(PathBuf::from(val)),
            "soundboard" => match val.parse::<SoundboardType>() {
                Ok(sb) => config.soundboard = sb,
                Err(_) => warn!("Unknown soundboard type in config: {val}"),
            },
            "adpcm-ram" => match val {
                "on" => config.adpcm_ram = true,
                "off" => config.adpcm_ram = false,
                _ => warn!("Invalid adpcm-ram in config: {val}, expected on or off"),
            },
            "gdc-compatibility" => match val {
                "on" => config.gdc_compatibility = true,
                "off" => config.gdc_compatibility = false,
                _ => warn!("Invalid gdc-compatibility in config: {val}, expected on or off"),
            },
            "printer" => config.printer = Some(PathBuf::from(val)),
            key if key.starts_with("key.") => {
                let host_name = &key[4..];
                match keymap::parse_key_binding(host_name, val) {
                    Some((host, pc98_code)) => config.key_map.set(host, pc98_code),
                    None => warn!("Invalid key binding: {key}={val}"),
                }
            }
            _ => warn!("Unknown config key: {key}"),
        }
    }

    Ok(config)
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum AspectMode {
    Aspect4By3,
    Aspect1By1,
}

impl std::fmt::Display for AspectMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Aspect4By3 => f.write_str("4:3"),
            Self::Aspect1By1 => f.write_str("1:1"),
        }
    }
}

/// Sound board type.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum SoundboardType {
    /// No sound board installed.
    None,
    /// PC-9801-26K only (YM2203 OPN).
    Sb26k,
    /// PC-9801-86 only (YM2608 OPNA + PCM86).
    Sb86,
    /// PC-9801-86 + PC-9801-26K (both boards).
    Sb86And26k,
    /// Creative Sound Blaster 16 (CT2720).
    Sb16,
    /// Creative Sound Blaster 16 (CT2720) + .
    Sb16And26k,
}

impl std::fmt::Display for SoundboardType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => f.write_str("none"),
            Self::Sb26k => f.write_str("26k"),
            Self::Sb86 => f.write_str("86"),
            Self::Sb86And26k => f.write_str("86+26k"),
            Self::Sb16 => f.write_str("sb16"),
            Self::Sb16And26k => f.write_str("sb16+26k"),
        }
    }
}

impl std::str::FromStr for SoundboardType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "none" => Ok(Self::None),
            "26k" => Ok(Self::Sb26k),
            "86" => Ok(Self::Sb86),
            "86+26k" => Ok(Self::Sb86And26k),
            "sb16" => Ok(Self::Sb16),
            "sb16+26k" => Ok(Self::Sb16And26k),
            _ => Err(format!(
                "unknown soundboard type '{s}', expected none, 26k, 86, 86+26k, sb16 or sb16+26k"
            )),
        }
    }
}

impl std::str::FromStr for AspectMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "4:3" => Ok(Self::Aspect4By3),
            "1:1" => Ok(Self::Aspect1By1),
            _ => Err(format!("unknown aspect mode '{s}', expected 4:3 or 1:1")),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum WindowMode {
    Windowed,
    Fullscreen,
}

impl std::fmt::Display for WindowMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Windowed => f.write_str("windowed"),
            Self::Fullscreen => f.write_str("fullscreen"),
        }
    }
}

impl std::str::FromStr for WindowMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "windowed" => Ok(Self::Windowed),
            "fullscreen" => Ok(Self::Fullscreen),
            _ => Err(format!(
                "unknown window mode '{s}', expected windowed or fullscreen"
            )),
        }
    }
}

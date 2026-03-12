use std::path::{Path, PathBuf};

use common::{Context, StringError, bail, warn};

fn next_value(flag: &str, args: &mut impl Iterator<Item = String>) -> crate::Result<String> {
    match args.next() {
        Some(val) => Ok(val),
        None => bail!("missing value for {flag}"),
    }
}

fn print_help() {
    println!(
        "\
{} — PC-98 emulator

Usage: neetan [OPTIONS]
       neetan <COMMAND>

Commands:
  create-fdd <PATH>           Create an empty floppy disk image (D88)
  create-hdd <PATH>           Create an empty hard disk image (HDI)

Options:
  -c, --config <PATH>         Load configuration from file
      --machine <TYPE>        Machine type: VM, VX, RA
      --fdd1 <PATH>           Floppy disk image for drive 1 (repeatable)
      --fdd2 <PATH>           Floppy disk image for drive 2 (repeatable)
      --hdd1 <PATH>           Hard disk image for SASI drive 1
      --hdd2 <PATH>           Hard disk image for SASI drive 2
      --audio-volume <FLOAT>  Audio volume 0.0-1.0
      --aspect-mode <MODE>    Display aspect mode: 4:3 or 1:1
      --bios-rom <PATH>       Path to BIOS ROM file
      --font-rom <PATH>       Path to font ROM file
      --soundboard <TYPE>     Sound board type: none, 26k, 86, 86+26k
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
      --type <TYPE>  HDD size [default: 40]
  -h, --help         Print help

HDD types (standard SASI geometries):
  5       5 MB  (153 cyl, 4 heads, 33 spt, 256 B/sector)
  10     10 MB  (310 cyl, 4 heads, 33 spt, 256 B/sector)
  15     15 MB  (310 cyl, 6 heads, 33 spt, 256 B/sector)
  20     20 MB  (310 cyl, 8 heads, 33 spt, 256 B/sector)
  30     30 MB  (615 cyl, 6 heads, 33 spt, 256 B/sector)
  40     40 MB  (615 cyl, 8 heads, 33 spt, 256 B/sector)"
    );
}

fn print_version() {
    println!("neetan {}", crate::CARGO_PKG_VERSION);
}

pub enum Action {
    Run(EmulatorConfig),
    CreateFdd {
        path: PathBuf,
        fdd_type: FddType,
    },
    CreateHdd {
        path: PathBuf,
        hdd_type: HddSizeType,
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
}

impl std::str::FromStr for HddSizeType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "5" => Ok(Self::Mb5),
            "10" => Ok(Self::Mb10),
            "15" => Ok(Self::Mb15),
            "20" => Ok(Self::Mb20),
            "30" => Ok(Self::Mb30),
            "40" => Ok(Self::Mb40),
            _ => Err(format!(
                "unknown HDD size '{s}', expected 5, 10, 15, 20, 30, or 40"
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
    let mut hdd_type = HddSizeType::Mb40;

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
                hdd_type = val.parse::<HddSizeType>().map_err(StringError)?;
            }
            other if !other.starts_with('-') && path.is_none() => {
                path = Some(PathBuf::from(other));
            }
            other => bail!("unknown argument: {other}"),
        }
    }

    let path = path.ok_or_else(|| StringError("missing required argument: <PATH>".into()))?;
    Ok(Action::CreateHdd { path, hdd_type })
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
                config.machine = val.parse::<MachineType>().map_err(StringError)?;
            }
            "--fdd1" => config.fdd1.push(PathBuf::from(value(&flag)?)),
            "--fdd2" => config.fdd2.push(PathBuf::from(value(&flag)?)),
            "--hdd1" => config.hdd1 = Some(PathBuf::from(value(&flag)?)),
            "--hdd2" => config.hdd2 = Some(PathBuf::from(value(&flag)?)),
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
            "--bios-rom" => config.bios_rom = Some(PathBuf::from(value(&flag)?)),
            "--font-rom" => config.font_rom = Some(PathBuf::from(value(&flag)?)),
            "--soundboard" => {
                let val = value(&flag)?;
                config.soundboard = val.parse::<SoundboardType>().map_err(StringError)?;
            }
            "--printer" => config.printer = Some(PathBuf::from(value(&flag)?)),
            other => bail!("unknown argument: {other}"),
        }
    }

    validate_paths(&config)?;

    Ok(Action::Run(config))
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
    if let Some(ref path) = config.hdd1 {
        if !path.exists() {
            bail!("hdd1 image not found: {}", path.display());
        }
    }
    if let Some(ref path) = config.hdd2 {
        if !path.exists() {
            bail!("hdd2 image not found: {}", path.display());
        }
    }
    Ok(())
}

pub struct EmulatorConfig {
    pub machine: MachineType,
    pub fdd1: Vec<PathBuf>,
    pub fdd2: Vec<PathBuf>,
    pub hdd1: Option<PathBuf>,
    pub hdd2: Option<PathBuf>,
    pub aspect_mode: AspectMode,
    pub audio_volume: f32,
    pub bios_rom: Option<PathBuf>,
    pub font_rom: Option<PathBuf>,
    pub soundboard: SoundboardType,
    pub printer: Option<PathBuf>,
}

impl Default for EmulatorConfig {
    fn default() -> Self {
        Self {
            machine: MachineType::RA,
            fdd1: Vec::new(),
            fdd2: Vec::new(),
            hdd1: None,
            hdd2: None,
            aspect_mode: AspectMode::Aspect4By3,
            audio_volume: 1.0,
            bios_rom: None,
            font_rom: None,
            soundboard: SoundboardType::Sb86And26k,
            printer: None,
        }
    }
}

pub fn parse_config_file(path: &Path) -> crate::Result<EmulatorConfig> {
    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;

    let mut config = EmulatorConfig::default();

    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, val)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let val = val.trim();
        match key {
            "machine" => match val.parse::<MachineType>() {
                Ok(mt) => config.machine = mt,
                Err(_) => warn!("Unknown machine type in config: {val}"),
            },
            "fdd1" => config.fdd1.push(PathBuf::from(val)),
            "fdd2" => config.fdd2.push(PathBuf::from(val)),
            "hdd1" => config.hdd1 = Some(PathBuf::from(val)),
            "hdd2" => config.hdd2 = Some(PathBuf::from(val)),
            "aspect-mode" => match val.parse::<AspectMode>() {
                Ok(mode) => config.aspect_mode = mode,
                Err(_) => warn!("Unknown aspect mode in config: {val}"),
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
            "printer" => config.printer = Some(PathBuf::from(val)),
            _ => warn!("Unknown config key: {key}"),
        }
    }

    Ok(config)
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum MachineType {
    /// PC-9801VM
    VM,
    /// PC-9801VX
    VX,
    /// PC-9801RA
    RA,
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

impl std::fmt::Display for MachineType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::VM => f.write_str("VM"),
            Self::VX => f.write_str("VX"),
            Self::RA => f.write_str("RA"),
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
}

impl std::fmt::Display for SoundboardType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => f.write_str("none"),
            Self::Sb26k => f.write_str("26k"),
            Self::Sb86 => f.write_str("86"),
            Self::Sb86And26k => f.write_str("86+26k"),
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
            _ => Err(format!(
                "unknown soundboard type '{s}', expected none, 26k, 86, or 86+26k"
            )),
        }
    }
}

impl std::str::FromStr for MachineType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_uppercase().as_str() {
            "VM" => Ok(Self::VM),
            "VX" => Ok(Self::VX),
            "RA" => Ok(Self::RA),
            _ => Err(format!(
                "unknown machine type '{s}', expected VM, VX, or RA"
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

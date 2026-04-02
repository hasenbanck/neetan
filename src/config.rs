use std::path::{Path, PathBuf};

use common::{Context, MachineModel, StringError, bail, info, warn};

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
{} - PC-98 emulator

Usage: neetan [OPTIONS]
       neetan <COMMAND>

Commands:
  create-fdd <PATH>             Create an empty floppy disk image (D88)
  create-hdd <PATH>             Create an empty hard disk image (HDI)
  convert-hdd <INPUT> <OUTPUT>  Convert HDD image between SASI and IDE

Options:
  -c, --config <PATH>           Load configuration from file
      --machine <TYPE>          Machine type: PC9801VM, PC9801VX, PC9801RA, PC9821AS, PC9821AP
      --fdd1 <PATH>             Floppy disk image for drive 1 (repeatable)
      --fdd2 <PATH>             Floppy disk image for drive 2 (repeatable)
      --hdd1 <PATH>             Hard disk image for drive 1 (SASI or IDE)
      --hdd2 <PATH>             Hard disk image for drive 2 (SASI or IDE)
      --cdrom <PATH>            CD-ROM disc image CUE file (repeatable, PC-9821 only)
      --audio-volume <FLOAT>    Audio volume 0.0-1.0
      --aspect-mode <MODE>      Display aspect mode: 4:3 or 1:1
      --window-mode <MODE>      Window mode: windowed or fullscreen
      --force-gdc-clock <2.5|5> Force GDC clock to 2.5 or 5 MHz (default: auto)
      --bios-rom <PATH>         Path to BIOS ROM file
      --font-rom <PATH>         Path to font ROM file
      --soundboard <TYPE>       Sound board type: none, 26k, 86, 86+26k, sb16, sb16+26k
      --adpcm-ram <on|off>      ADPCM RAM option for PC-9801-86 (default: on)
      --midi <DEVICE>           MIDI device: none, mt32, sc55 (default: none)
      --mt32-roms <PATH>        Path to MT-32 ROM directory
      --sc55-roms <PATH>        Path to SC55 ROM directory
      --printer <PATH>          Output file for printer (must exist)
  -h, --help                    Print help
  -V, --version                 Print version

Global configuration:
  A global config is loaded from the OS data directory if it exists.
  Layering: defaults -> global config -> --config file -> CLI arguments

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

    if let Some(global_path) = global_config_path()
        && global_path.exists()
    {
        apply_config_file(&mut config, &global_path)?;
        info!("Loaded global config: {}", global_path.display());
    }

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
                apply_config_file(&mut config, Path::new(&path))?;
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
            "--force-gdc-clock" => {
                let val = value(&flag)?;
                config.force_gdc_clock = Some(val.parse::<ForceGdcClock>().map_err(StringError)?);
            }
            "--printer" => config.printer = Some(PathBuf::from(value(&flag)?)),
            "--mt32-roms" => config.mt32_roms = Some(PathBuf::from(value(&flag)?)),
            "--sc55-roms" => config.sc55_roms = Some(PathBuf::from(value(&flag)?)),
            "--midi" => {
                let val = value(&flag)?;
                config.midi = val.parse::<MidiDevice>().map_err(StringError)?;
            }
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
    pub force_gdc_clock: Option<ForceGdcClock>,
    pub printer: Option<PathBuf>,
    pub mt32_roms: Option<PathBuf>,
    pub sc55_roms: Option<PathBuf>,
    pub midi: MidiDevice,
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
            force_gdc_clock: None,
            printer: None,
            mt32_roms: None,
            sc55_roms: None,
            midi: MidiDevice::default(),
            key_map: KeyMap::new(),
        }
    }
}

pub fn parse_config_file(path: &Path) -> crate::Result<EmulatorConfig> {
    let mut config = EmulatorConfig::default();
    apply_config_file(&mut config, path)?;
    Ok(config)
}

fn apply_config_file(config: &mut EmulatorConfig, path: &Path) -> crate::Result<()> {
    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;

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
            "force-gdc-clock" => match val.parse::<ForceGdcClock>() {
                Ok(mode) => config.force_gdc_clock = Some(mode),
                Err(_) => warn!("Invalid force-gdc-clock in config: {val}, expected 2.5 or 5"),
            },
            "printer" => config.printer = Some(PathBuf::from(val)),
            "mt32-roms" => config.mt32_roms = Some(PathBuf::from(val)),
            "sc55-roms" => config.sc55_roms = Some(PathBuf::from(val)),
            "midi" => match val.parse::<MidiDevice>() {
                Ok(device) => config.midi = device,
                Err(_) => warn!("Unknown MIDI device in config: {val}"),
            },
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

    Ok(())
}

fn global_config_path() -> Option<PathBuf> {
    let pref_path = sdl3::filesystem::get_pref_path(crate::COMPANY_NAME, crate::GAME_NAME)?;
    Some(pref_path.join("global.conf"))
}

/// Forced GDC clock speed.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ForceGdcClock {
    /// Force 2.5 MHz (200-line compatibility mode).
    Force2_5,
    /// Force 5 MHz (400-line graphics mode).
    Force5,
}

impl std::fmt::Display for ForceGdcClock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Force2_5 => f.write_str("2.5"),
            Self::Force5 => f.write_str("5"),
        }
    }
}

impl std::str::FromStr for ForceGdcClock {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "2.5" => Ok(Self::Force2_5),
            "5" => Ok(Self::Force5),
            _ => Err(format!("unknown GDC clock mode '{s}', expected 2.5 or 5")),
        }
    }
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

/// MIDI output device.
#[derive(Debug, Copy, Clone, Default, PartialEq, Eq)]
pub enum MidiDevice {
    /// No MIDI output.
    #[default]
    None,
    /// Roland MT-32 (requires MT-32 ROMs).
    Mt32,
    /// Roland SC-55 (requires SC-55 ROMs).
    Sc55,
}

impl std::fmt::Display for MidiDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => f.write_str("none"),
            Self::Mt32 => f.write_str("mt32"),
            Self::Sc55 => f.write_str("sc55"),
        }
    }
}

impl std::str::FromStr for MidiDevice {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "none" => Ok(Self::None),
            "mt32" => Ok(Self::Mt32),
            "sc55" => Ok(Self::Sc55),
            _ => Err(format!(
                "unknown MIDI device '{s}', expected none, mt32 or sc55"
            )),
        }
    }
}

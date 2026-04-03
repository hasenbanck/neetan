//! Neetan, a PC-98 emulator.

#![deny(unsafe_code)]

use std::{
    fs::File,
    time::{Duration, Instant},
};

use audio_engine::AudioEngine;
use common::{Context, Machine, MachineModel, StringError, ensure, error, info, warn};
use device::disk::{HddGeometry, load_hdd_image};
use graphics_engine::{DisplayAspectMode, GraphicsEngine, RenderInstructions};
use jay_ash::{vk, vk::Handle};
use sdl3::{
    Sdl,
    audio::AudioSubsystem,
    event::{DisplayEvent, Event, WindowEvent},
    keyboard::Scancode,
    mouse::MouseButton,
    video::{VideoSubsystem, Window},
};

use crate::{
    config::{AspectMode, EmulatorConfig, ForceGdcClock, WindowMode},
    errors::Error,
    image_selector::{ImageEntry, ImageSelector, MediaType},
};

pub mod config;
pub mod convert;
pub mod create;
mod errors;
mod image_selector;
mod keymap;

#[cfg(feature = "tracing")]
mod tracing;

#[cfg(feature = "tracing")]
type Tracer = crate::tracing::Tracing;
#[cfg(not(feature = "tracing"))]
type Tracer = machine::NoTracing;

pub const COMPANY_NAME: &str = "neetan";
pub const GAME_NAME: &str = "neetan";
pub const CARGO_PKG_VERSION: &str = env!("CARGO_PKG_VERSION");
const INITIAL_WINDOW_WIDTH: u32 = 1280;
const MAX_AUDIO_STEPS: usize = 40;
const SAMPLE_RATE: f64 = audio_engine::SAMPLE_RATE as f64;

pub type Result<T> = std::result::Result<T, Error>;

pub fn run(config: EmulatorConfig) -> Result<()> {
    let aspect_mode = config.aspect_mode;
    let (initial_width, initial_height) = initial_window_size(aspect_mode);
    let aspect_ratio = aspect_ratio_for_mode(aspect_mode);
    let graphics_display_aspect_mode = graphics_display_aspect_mode(aspect_mode);

    let (sdl_context, audio_subsystem, video_subsystem) = initialize_sdl3()?;

    print_system_into();

    let mut builder = video_subsystem
        .window(GAME_NAME, initial_width, initial_height)
        .high_pixel_density()
        .resizable()
        .position_centered()
        .hidden()
        .vulkan();

    if config.window_mode == WindowMode::Fullscreen {
        builder = builder.fullscreen();
    }

    let mut window = builder
        .build()
        .context("Failed to create window with SDL3")?;

    if config.window_mode != WindowMode::Fullscreen
        && let Err(error) = window.set_aspect_ratio(aspect_ratio)
    {
        warn!("Failed to lock window aspect ratio to {aspect_ratio}: {error}");
    }

    let (width, height) = window.size();
    let (pixel_width, pixel_height) = window.size_in_pixels();

    let platform_extension_names = window
        .vulkan_instance_extensions()
        .context("SDL_Vulkan_GetInstanceExtensions failed")?;

    let mut application = Application::new(
        config,
        audio_subsystem,
        &window,
        platform_extension_names.as_slice(),
        graphics_display_aspect_mode,
        (width as f32, height as f32),
    )?;

    let surface_handle = create_surface(&mut window, &mut application)?;

    application
        .graphics_engine
        .on_resume(surface_handle, true, pixel_width, pixel_height);

    window.show();

    let (pixel_width, pixel_height) = window.size_in_pixels();
    if let Err(error) = application
        .graphics_engine
        .on_resize(pixel_width, pixel_height)
    {
        error!("Error on initial resize after show: {error}");
    }

    let mut event_pump = sdl_context
        .event_pump()
        .context("Failed to get the SDL3 event pump")?;

    'running: loop {
        for event in event_pump.poll_iter() {
            if application.handle_event(&event, Some(&window)) {
                break 'running;
            }
        }

        let busy_start = Instant::now();
        application.run_emulation();
        application.busy_duration += busy_start.elapsed();

        let gpu_ready = application
            .graphics_engine
            .try_wait_for_previous_present(1)
            .unwrap_or(true);

        if gpu_ready && let Err(error) = application.render_frame() {
            error!("Failed to render next frame: {error:#}");
        }

        let elapsed = application.window_title_last_update.elapsed();
        if elapsed >= Duration::from_secs(5) {
            let busy_percent = (application.busy_duration.as_secs_f64() / elapsed.as_secs_f64()
                * 100.0)
                .round()
                .min(100.0) as u32;
            window.set_title(&format!("neetan ({busy_percent}% CPU)"));
            application.busy_duration = Duration::ZERO;
            application.window_title_last_update = Instant::now();
        }

        if application.should_quit {
            break 'running;
        }
    }

    Ok(())
}

fn print_system_into() {
    let (sdl3_major, sdl3_minor, sdl3_patch) = sdl3::info::version();
    let sdl3_revision = sdl3::info::revision();
    info!("SDL3 v{sdl3_major}.{sdl3_minor}.{sdl3_patch} ({sdl3_revision})");
    let platform = sdl3::info::platform();
    info!("Running on {platform}");
    let cpu = sdl3::info::num_logical_cpu_cores();
    info!("System has {cpu} CPU(s)");
    let system_ram_mib = sdl3::info::system_ram();
    info!("System has {system_ram_mib} MiB");
}

fn initialize_sdl3() -> Result<(Sdl, AudioSubsystem, VideoSubsystem)> {
    let sdl_context = sdl3::init().context("Failed to initialize SDL3")?;

    sdl3::log::set_log_priorities(sdl3::log::LogPriority::Verbose);
    sdl3::log::set_log_output_function(sdl3_log_callback);

    let audio_subsystem = sdl_context
        .audio()
        .context("Failed to initialize SDL3 audio subsystem")?;

    let video_subsystem = sdl_context
        .video()
        .context("Failed to initialize SDL3 video subsystem")?;

    #[cfg(target_os = "macos")]
    load_vulkan_library(&video_subsystem)?;

    Ok((sdl_context, audio_subsystem, video_subsystem))
}

fn sdl3_log_callback(_category: i32, priority: sdl3::log::LogPriority, message: &str) {
    let level = match priority {
        sdl3::log::LogPriority::Trace | sdl3::log::LogPriority::Verbose => {
            common::log::Level::Trace
        }
        sdl3::log::LogPriority::Debug => common::log::Level::Debug,
        sdl3::log::LogPriority::Info => common::log::Level::Info,
        sdl3::log::LogPriority::Warn => common::log::Level::Warn,
        sdl3::log::LogPriority::Error | sdl3::log::LogPriority::Critical => {
            common::log::Level::Error
        }
    };
    common::log::log_record(level, "sdl3", format_args!("{message}"));
}

#[cfg(target_os = "macos")]
fn load_vulkan_library(video_subsystem: &VideoSubsystem) -> Result<()> {
    use std::ffi::CString;

    let c_path = if let Ok(sdk) = std::env::var("VULKAN_SDK") {
        let lib = format!("{sdk}/lib/libvulkan.1.dylib");
        Some(CString::new(lib).map_err(|e| Error::Message(StringError(e.to_string())))?)
    } else {
        None
    };

    video_subsystem
        .load_vulkan_library(c_path.as_deref())
        .map_err(|error| -> Error {
            StringError(format!(
                "Failed to load Vulkan library: {error}. \
                 Install the LunarG Vulkan SDK and set VULKAN_SDK in your environment."
            ))
            .into()
        })
}

fn initial_window_size(aspect_mode: AspectMode) -> (u32, u32) {
    let initial_height = match aspect_mode {
        AspectMode::Aspect4By3 => 960,
        AspectMode::Aspect1By1 => 800,
    };
    (INITIAL_WINDOW_WIDTH, initial_height)
}

fn aspect_ratio_for_mode(aspect_mode: AspectMode) -> f32 {
    match aspect_mode {
        AspectMode::Aspect4By3 => 4.0 / 3.0,
        AspectMode::Aspect1By1 => 16.0 / 10.0,
    }
}

fn graphics_display_aspect_mode(aspect_mode: AspectMode) -> DisplayAspectMode {
    match aspect_mode {
        AspectMode::Aspect4By3 => DisplayAspectMode::Aspect4By3,
        AspectMode::Aspect1By1 => DisplayAspectMode::Aspect1By1,
    }
}

fn create_surface(window: &mut Window, application: &mut Application) -> Result<vk::SurfaceKHR> {
    // TODO: We have access to both our graphics engine and also the SDL3 crate, so we should find
    //       a way to move the unsafe code into them.
    let instance_handle = application.graphics_engine.raw_instance_handle();
    let sdl_instance = instance_handle.as_raw() as sdl3::video::VkInstance;
    // Safety: The graphics engine ensures the Vulkan instance is valid.
    #[allow(unsafe_code)]
    let sdl_surface = unsafe { window.vulkan_create_surface(sdl_instance) }
        .context("SDL_Vulkan_CreateSurface failed")?;
    let surface_handle = vk::SurfaceKHR::from_raw(sdl_surface as u64);

    Ok(surface_handle)
}

struct Application {
    /// The emulated machine.
    machine: Box<dyn Machine>,
    /// The graphics engine.
    graphics_engine: GraphicsEngine,
    /// Audio engine which outputs using the SDL3 push-based stream. Drives emulation speed.
    audio_engine: AudioEngine,
    /// The speed of the CPU on cycles per second.
    cpu_hz: f64,
    /// Tracks CPU cycle overshoot from previous audio steps for precise timing.
    cycle_overshoot: u64,
    /// Current logical viewport size.
    logical_size: (f32, f32),
    /// Current display scale factor for UI scaling.
    scale_factor: f32,
    /// Whether we should quit.
    should_quit: bool,
    /// Accumulated mouse X delta since last frame sync (sub-pixel).
    mouse_dx: f32,
    /// Accumulated mouse Y delta since last frame sync (sub-pixel).
    mouse_dy: f32,
    /// Current mouse button state.
    mouse_left: bool,
    mouse_right: bool,
    mouse_middle: bool,
    /// Whether relative mouse mode is active (Right Ctrl toggles).
    mouse_captured: bool,
    /// Host-to-PC-98 key mapping.
    key_map: keymap::KeyMap,
    /// Floppy disk image entries for drive 1.
    fdd1_entries: Vec<ImageEntry>,
    /// Current index into fdd1_entries, or `None` if no floppy is loaded.
    fdd1_index: Option<usize>,
    /// Floppy disk image entries for drive 2.
    fdd2_entries: Vec<ImageEntry>,
    /// Current index into fdd2_entries, or `None` if no floppy is loaded.
    fdd2_index: Option<usize>,
    /// CD-ROM disc image entries.
    cdrom_entries: Vec<ImageEntry>,
    /// Current index into cdrom_entries, or `None` if no disc is loaded.
    cdrom_index: Option<usize>,
    /// Active image selection screen, if open.
    image_selector: Option<ImageSelector>,
    /// Whether the window is currently in fullscreen mode.
    fullscreen: bool,
    /// Accumulated emulation busy time in the current measurement window.
    busy_duration: Duration,
    /// When the window title was last updated with CPU usage.
    window_title_last_update: Instant,
}

impl Drop for Application {
    fn drop(&mut self) {
        self.machine.flush_printer();
        self.machine.flush_floppies();
        self.machine.flush_hdds();
    }
}

impl Application {
    pub(crate) fn new(
        config: EmulatorConfig,
        audio_subsystem: AudioSubsystem,
        window: &Window,
        platform_extensions: &[String],
        display_aspect_mode: DisplayAspectMode,
        logical_size: (f32, f32),
    ) -> Result<Self> {
        let audio_engine = AudioEngine::new(audio_subsystem, config.audio_volume)
            .context("Failed to initialize audio")?;

        let fdd1_entries: Vec<ImageEntry> =
            config.fdd1.iter().cloned().map(ImageEntry::new).collect();
        let fdd2_entries: Vec<ImageEntry> =
            config.fdd2.iter().cloned().map(ImageEntry::new).collect();
        let cdrom_entries: Vec<ImageEntry> =
            config.cdrom.iter().cloned().map(ImageEntry::new).collect();

        let mut machine = initialize_machine(&config, audio_engine::SAMPLE_RATE as u32)?;
        let key_map = config.key_map;

        let mut fdd1_index = None;
        if let Some(entry) = fdd1_entries.first() {
            match machine.insert_floppy(0, &entry.path) {
                Ok(desc) => {
                    info!("Inserted FDD1: {desc} from {}", entry.path.display());
                    fdd1_index = Some(0);
                }
                Err(e) => return Err(Error::from(StringError(e))),
            }
        }
        let mut fdd2_index = None;
        if let Some(entry) = fdd2_entries.first() {
            match machine.insert_floppy(1, &entry.path) {
                Ok(desc) => {
                    info!("Inserted FDD2: {desc} from {}", entry.path.display());
                    fdd2_index = Some(0);
                }
                Err(e) => return Err(Error::from(StringError(e))),
            }
        }
        let mut cdrom_index = None;
        if let Some(entry) = cdrom_entries.first() {
            match machine.insert_cdrom(&entry.path) {
                Ok(desc) => {
                    info!("Inserted CD-ROM: {desc} from {}", entry.path.display());
                    cdrom_index = Some(0);
                }
                Err(e) => return Err(Error::from(StringError(e))),
            }
        }

        let cpu_hz = machine.cpu_clock_hz();

        let graphics_engine = GraphicsEngine::new(
            platform_extensions,
            machine.font_rom_data(),
            display_aspect_mode,
        )
        .context("Failed to create graphics engine")?;

        let scale_factor = window.display_scale();

        info!("Window created with scale factor: {scale_factor}");

        Ok(Self {
            machine,
            audio_engine,
            cpu_hz,
            cycle_overshoot: 0,
            logical_size,
            scale_factor,
            graphics_engine,
            should_quit: false,
            mouse_dx: 0.0,
            mouse_dy: 0.0,
            mouse_left: false,
            mouse_right: false,
            mouse_middle: false,
            mouse_captured: false,
            key_map,
            fdd1_entries,
            fdd1_index,
            fdd2_entries,
            fdd2_index,
            cdrom_entries,
            cdrom_index,
            image_selector: None,
            fullscreen: config.window_mode == WindowMode::Fullscreen,
            busy_duration: Duration::ZERO,
            window_title_last_update: Instant::now(),
        })
    }

    /// Handles the most important window and keyboard events.
    fn handle_event(&mut self, event: &Event, window: Option<&Window>) -> bool {
        match event {
            Event::Quit => {
                self.should_quit = true;
                return true;
            }
            Event::Window {
                win_event: WindowEvent::Resized(width, height),
                ..
            } => {
                // Resized is in logical unit.
                let width = *width as u32;
                let height = *height as u32;
                let logical_size = (width as f32, height as f32);
                let physical_size = (
                    (width as f32 * self.scale_factor) as u32,
                    (height as f32 * self.scale_factor) as u32,
                );

                if let Err(error) = self
                    .graphics_engine
                    .on_resize(physical_size.0, physical_size.1)
                {
                    error!("Error on resize: {error}");
                }

                self.logical_size = logical_size;
            }
            Event::Window {
                win_event: WindowEvent::PixelSizeChanged(width, height),
                ..
            } => {
                // PixelSizeChanged is in physical unit.
                let width = *width as u32;
                let height = *height as u32;
                let logical_size = (
                    width as f32 / self.scale_factor,
                    height as f32 / self.scale_factor,
                );
                let physical_size = (width, height);

                if let Err(error) = self
                    .graphics_engine
                    .on_resize(physical_size.0, physical_size.1)
                {
                    error!("Error on resize: {error}");
                }

                self.logical_size = logical_size;
            }
            Event::Window {
                win_event: WindowEvent::FocusLost,
                ..
            } => {
                self.audio_engine.pause();
            }
            Event::Window {
                win_event: WindowEvent::FocusGained,
                ..
            } => {
                if self.image_selector.is_none() {
                    self.audio_engine.resume();
                }
            }
            Event::Display {
                display_event: DisplayEvent::ContentScaleChanged,
                ..
            } => {
                if let Some(scale) = window.map(|w| w.display_scale()) {
                    self.scale_factor = scale;
                    info!("Scale factor changed to: {scale}");
                }
            }
            Event::KeyDown {
                scancode,
                keymod,
                repeat,
                ..
            } => {
                if self.image_selector.is_some() {
                    if !repeat {
                        self.handle_selector_key(*scancode, keymod.alt_gui());
                    }
                } else {
                    // Right Ctrl toggles mouse capture.
                    if !repeat
                        && *scancode == Some(Scancode::RCtrl)
                        && let Some(w) = window
                    {
                        self.toggle_mouse_capture(w);
                    }

                    if !repeat && keymod.alt_gui() && *scancode == Some(Scancode::Escape) {
                        self.should_quit = true;
                    } else if !repeat && keymod.alt_gui() && *scancode == Some(Scancode::Return) {
                        if let Some(w) = window {
                            if let Err(error) = w.set_fullscreen(!self.fullscreen) {
                                warn!("Failed to toggle fullscreen: {error}");
                            } else {
                                self.fullscreen = !self.fullscreen;
                            }
                        }
                    } else if !repeat && keymod.alt_gui() && *scancode == Some(Scancode::F9) {
                        self.open_or_toggle_selector(MediaType::Floppy(0));
                    } else if !repeat && keymod.alt_gui() && *scancode == Some(Scancode::F10) {
                        self.open_or_toggle_selector(MediaType::Floppy(1));
                    } else if !repeat && keymod.alt_gui() && *scancode == Some(Scancode::F11) {
                        self.open_or_toggle_selector(MediaType::CdRom);
                    } else if !repeat
                        && let Some(code) = (*scancode).map(|sc| self.key_map.lookup(sc))
                    {
                        self.machine.push_keyboard_scancode(code);
                    }
                }
            }
            Event::KeyUp {
                scancode, repeat, ..
            } => {
                if self.image_selector.is_none()
                    && !repeat
                    && let Some(code) = (*scancode).map(|sc| self.key_map.lookup(sc))
                {
                    self.machine.push_keyboard_scancode(code | 0x80);
                }
            }
            Event::MouseMotion { xrel, yrel, .. } => {
                if self.mouse_captured {
                    self.mouse_dx += xrel;
                    self.mouse_dy += yrel;
                }
            }
            Event::MouseButtonDown { mouse_btn, .. } => {
                if self.mouse_captured {
                    match mouse_btn {
                        MouseButton::Left => self.mouse_left = true,
                        MouseButton::Right => self.mouse_right = true,
                        MouseButton::Middle => self.mouse_middle = true,
                        _ => {}
                    }
                    self.machine.set_mouse_buttons(
                        self.mouse_left,
                        self.mouse_right,
                        self.mouse_middle,
                    );
                }
            }
            Event::MouseButtonUp { mouse_btn, .. } => {
                if self.mouse_captured {
                    match mouse_btn {
                        MouseButton::Left => self.mouse_left = false,
                        MouseButton::Right => self.mouse_right = false,
                        MouseButton::Middle => self.mouse_middle = false,
                        _ => {}
                    }
                    self.machine.set_mouse_buttons(
                        self.mouse_left,
                        self.mouse_right,
                        self.mouse_middle,
                    );
                }
            }
            _ => {}
        }

        false
    }

    /// Toggles mouse capture (relative mouse mode) on the given window.
    fn toggle_mouse_capture(&mut self, window: &Window) {
        let desired = !self.mouse_captured;

        if let Err(error) = window.set_relative_mouse_mode(desired) {
            warn!("Failed to set relative mouse mode: {error}");
            return;
        }

        self.mouse_captured = desired;

        if !self.mouse_captured {
            // Release all buttons when uncapturing.
            self.mouse_left = false;
            self.mouse_right = false;
            self.mouse_middle = false;
            self.machine.set_mouse_buttons(false, false, false);
            self.mouse_dx = 0.0;
            self.mouse_dy = 0.0;
        }

        info!(
            "Mouse {}",
            if self.mouse_captured {
                "captured"
            } else {
                "released"
            }
        );
    }

    fn eject_floppy(&mut self, drive: usize) {
        self.machine.eject_floppy(drive);
        match drive {
            0 => self.fdd1_index = None,
            1 => self.fdd2_index = None,
            _ => {}
        }
        info!("Ejected FDD{}", drive + 1);
    }

    fn select_floppy(&mut self, drive: usize, index: usize) {
        let entries = match drive {
            0 => &self.fdd1_entries,
            1 => &self.fdd2_entries,
            _ => return,
        };

        if index >= entries.len() {
            return;
        }

        self.machine.eject_floppy(drive);

        let path = &entries[index].path;
        match self.machine.insert_floppy(drive, path) {
            Ok(desc) => info!("Selected FDD{}: {desc} from {}", drive + 1, path.display()),
            Err(error) => error!("Failed to select FDD{}: {error}", drive + 1),
        }

        match drive {
            0 => self.fdd1_index = Some(index),
            1 => self.fdd2_index = Some(index),
            _ => {}
        }
    }

    fn eject_cdrom(&mut self) {
        self.machine.eject_cdrom();
        self.cdrom_index = None;
        info!("Ejected CD-ROM");
    }

    fn select_cdrom(&mut self, index: usize) {
        if index >= self.cdrom_entries.len() {
            return;
        }

        self.machine.eject_cdrom();

        let path = &self.cdrom_entries[index].path;
        match self.machine.insert_cdrom(path) {
            Ok(desc) => info!("Selected CD-ROM: {desc} from {}", path.display()),
            Err(error) => error!("Failed to select CD-ROM: {error}"),
        }

        self.cdrom_index = Some(index);
    }

    fn open_or_toggle_selector(&mut self, media_type: MediaType) {
        if let Some(ref selector) = self.image_selector
            && *selector.media_type() == media_type
        {
            self.close_selector();
            return;
        }

        let (entries, current_index) = match &media_type {
            MediaType::Floppy(0) => (&self.fdd1_entries, self.fdd1_index),
            MediaType::Floppy(_) => (&self.fdd2_entries, self.fdd2_index),
            MediaType::CdRom => (&self.cdrom_entries, self.cdrom_index),
        };

        // Display position: None (empty) -> 0, Some(n) -> n + 1.
        let display_cursor = current_index.map_or(0, |n| n + 1);
        let display_count = entries.len() + 1;

        if let Some(ref mut selector) = self.image_selector {
            selector.switch_media(media_type, display_cursor, display_count);
        } else {
            self.audio_engine.pause();
            self.image_selector = Some(ImageSelector::new(
                media_type,
                display_cursor,
                display_count,
            ));
        }
    }

    fn close_selector(&mut self) {
        self.image_selector = None;
        self.audio_engine.resume();
    }

    fn handle_selector_key(&mut self, scancode: Option<Scancode>, alt_gui_held: bool) {
        let Some(code) = scancode else { return };

        match code {
            Scancode::Up => {
                if let Some(ref mut selector) = self.image_selector {
                    selector.move_up();
                }
            }
            Scancode::Down => {
                if let Some(ref mut selector) = self.image_selector {
                    let count = match selector.media_type() {
                        MediaType::Floppy(0) => self.fdd1_entries.len() + 1,
                        MediaType::Floppy(_) => self.fdd2_entries.len() + 1,
                        MediaType::CdRom => self.cdrom_entries.len() + 1,
                    };
                    selector.move_down(count);
                }
            }
            Scancode::Return | Scancode::KpEnter => {
                if let Some(ref selector) = self.image_selector {
                    let media_type = selector.media_type().clone();
                    let cursor = selector.cursor();
                    match &media_type {
                        MediaType::Floppy(drive) => {
                            if cursor == 0 {
                                self.eject_floppy(*drive);
                            } else {
                                self.select_floppy(*drive, cursor - 1);
                            }
                        }
                        MediaType::CdRom => {
                            if cursor == 0 {
                                self.eject_cdrom();
                            } else {
                                self.select_cdrom(cursor - 1);
                            }
                        }
                    }
                }
                self.close_selector();
            }
            Scancode::Escape => {
                self.close_selector();
            }
            Scancode::F9 if alt_gui_held => {
                self.open_or_toggle_selector(MediaType::Floppy(0));
            }
            Scancode::F10 if alt_gui_held => {
                self.open_or_toggle_selector(MediaType::Floppy(1));
            }
            Scancode::F11 if alt_gui_held => {
                self.open_or_toggle_selector(MediaType::CdRom);
            }
            _ => {}
        }
    }

    fn run_emulation(&mut self) {
        if self.image_selector.is_some() {
            return;
        }

        // Flush accumulated mouse movement into the emulated machine.
        if self.mouse_captured {
            let dx = self.mouse_dx.round() as i16;
            let dy = self.mouse_dy.round() as i16;
            self.machine.push_mouse_delta(dx, dy);
            self.mouse_dx = 0.0;
            self.mouse_dy = 0.0;
        }

        for _ in 0..MAX_AUDIO_STEPS {
            let needed_frames = self.audio_engine.frames_needed() as u64;
            if needed_frames == 0 {
                break;
            }
            let raw_cycles = (needed_frames as f64 * self.cpu_hz / SAMPLE_RATE).round() as u64;

            // If a previous step overshot by more cycles than this step needs,
            // consume only what's needed and carry the remainder to avoid timing drift.
            if self.cycle_overshoot >= raw_cycles {
                self.cycle_overshoot -= raw_cycles;
                self.audio_engine.push_samples(self.machine.as_mut());
                continue;
            }

            let cycles = raw_cycles - self.cycle_overshoot;
            self.cycle_overshoot = 0;

            let ran_cycles = self.machine.run_for(cycles);
            if ran_cycles > cycles {
                self.cycle_overshoot = ran_cycles - cycles;
            }
            self.audio_engine.push_samples(self.machine.as_mut());

            if self.machine.shutdown_requested() {
                info!("Guest triggered system shutdown");
                self.should_quit = true;
                return;
            }
        }
    }

    fn render_frame(&mut self) -> Result<()> {
        if self.machine.take_font_rom_dirty() {
            self.graphics_engine
                .update_font_rom(self.machine.font_rom_data());
        }

        let display_snapshot = if let Some(ref mut selector) = self.image_selector {
            let (entries, loaded_index) = match selector.media_type() {
                MediaType::Floppy(0) => (&self.fdd1_entries, self.fdd1_index),
                MediaType::Floppy(_) => (&self.fdd2_entries, self.fdd2_index),
                MediaType::CdRom => (&self.cdrom_entries, self.cdrom_index),
            };
            selector.ensure_snapshot(entries, loaded_index);
            selector.snapshot()
        } else {
            self.machine.snapshot_display()
        };

        let pegc_snapshot = self.machine.pegc_snapshot_display();

        self.graphics_engine
            .render_frame(Some(&RenderInstructions {
                display_snapshot,
                pegc_snapshot,
            }))
            .context("Graphics engine failed to render frame")?;

        Ok(())
    }
}

/// Returns the current host local time as a 6-byte BCD buffer for the µPD4990A RTC.
///
/// Format: `[year, month<<4|day_of_week, day, hour, minute, second]`.
fn host_local_time_bcd() -> [u8; 6] {
    fn to_bcd(value: u8) -> u8 {
        ((value / 10) << 4) | (value % 10)
    }

    let Ok(dt) = sdl3::time::local_date_time() else {
        return [0; 6];
    };

    let year = to_bcd((dt.year % 100) as u8);
    let month_dow = ((dt.month as u8) << 4) | (dt.day_of_week as u8);
    let day = to_bcd(dt.day as u8);
    let hour = to_bcd(dt.hour as u8);
    let minute = to_bcd(dt.minute as u8);
    let second = to_bcd(dt.second as u8);

    [year, month_dow, day, hour, minute, second]
}

fn initialize_machine(config: &EmulatorConfig, sample_rate: u32) -> Result<Box<dyn Machine>> {
    let model = config.machine;

    info!("Selected machine model {model}");

    let mut bus: machine::Pc9801Bus<Tracer> = machine::Pc9801Bus::new(model, sample_rate);
    bus.set_host_local_time_fn(host_local_time_bcd);

    // GDC clock rate configuration logic
    match (model.has_pegc(), model.has_egc(), config.force_gdc_clock) {
        // PEGC machines (PC-9821): default to 5 MHz
        (true, _, None) => {
            bus.set_gdc_clock_5mhz();
        }
        (true, _, Some(ForceGdcClock::Force5)) => {
            bus.set_gdc_clock_5mhz();
            info!("GDC clock forced to 5 MHz (400-line graphics mode)");
        }
        (true, _, Some(ForceGdcClock::Force2_5)) => {
            info!("GDC clock forced to 2.5 MHz (200-line compatibility mode)");
        }
        // EGC-only machines (PC-9801VX/RA): default to 2.5 MHz
        (false, true, Some(ForceGdcClock::Force5)) => {
            bus.set_gdc_clock_5mhz();
            info!("GDC clock forced to 5 MHz (400-line graphics mode)");
        }
        (false, true, Some(ForceGdcClock::Force2_5)) => {
            info!("GDC clock forced to 2.5 MHz (200-line compatibility mode)");
        }
        (false, true, None) => {}
        // Non-EGC machines (PC-9801VM): no 5 MHz support
        (false, false, Some(ForceGdcClock::Force5)) => {
            warn!("{model} does not support 5 MHz GDC clock, ignoring --force-gdc-clock 5");
        }
        (false, false, Some(ForceGdcClock::Force2_5)) | (false, false, None) => {}
    }

    if config.bios_rom.is_some() && model.is_pc9821() {
        warn!("Real BIOS ROM is not supported for PC-9821. Use HLE BIOS mode (omit --bios-rom).");
    }

    if let Some(ref bios_path) = config.bios_rom {
        let bios_rom = std::fs::read(bios_path)
            .with_context(|| format!("Failed to read BIOS ROM from {}", bios_path.display()))?;

        let expected = model.bios_rom_size();
        ensure!(
            bios_rom.len() == expected,
            "BIOS ROM is {} bytes, expected exactly {} bytes for {}: {}",
            bios_rom.len(),
            expected,
            model,
            bios_path.display()
        );

        info!(
            "Loaded BIOS ROM ({} bytes) from {}",
            bios_rom.len(),
            bios_path.display()
        );
        bus.load_bios_rom(&bios_rom);
    } else {
        info!("No BIOS ROM provided - running in HLE BIOS mode");
    }

    match config.font_rom {
        Some(ref font_path) => match std::fs::read(font_path) {
            Ok(font_rom) => {
                info!(
                    "Loaded font ROM ({} bytes) from {}",
                    font_rom.len(),
                    font_path.display()
                );
                bus.load_font_rom(&font_rom);
            }
            Err(error) => {
                error!(
                    "Failed to read font ROM from {}: {error}",
                    font_path.display()
                );
            }
        },
        None => {
            const BUILTIN_FONT_ROM: &[u8] = include_bytes!("../utils/font/font.rom");
            info!("Using built-in font ROM ({} bytes)", BUILTIN_FONT_ROM.len());
            bus.load_font_rom(BUILTIN_FONT_ROM);
        }
    }

    match config.soundboard {
        config::SoundboardType::None => {}
        config::SoundboardType::Sb26k => {
            bus.install_soundboard_26k(false);
            info!("Installed PC-9801-26K sound board (YM2203 OPN)");
        }
        config::SoundboardType::Sb86 => {
            bus.install_soundboard_86(None, config.adpcm_ram);
            info!("Installed PC-9801-86 sound board (YM2608 OPNA + PCM86)");
        }
        config::SoundboardType::Sb86And26k => {
            bus.install_soundboard_26k(true);
            info!("Installed PC-9801-26K sound board (YM2203 OPN) at alternate ports");
            bus.install_soundboard_86(None, config.adpcm_ram);
            info!("Installed PC-9801-86 sound board (YM2608 OPNA + PCM86)");
        }
        config::SoundboardType::Sb16 => {
            bus.install_sound_blaster_16();
            info!("Installed Sound Blaster 16 (CT2720, YMF262 OPL3 + CT1741 DSP)");
        }
        config::SoundboardType::Sb16And26k => {
            bus.install_soundboard_26k(false);
            info!("Installed PC-9801-26K sound board (YM2203 OPN)");
            bus.install_sound_blaster_16();
            info!("Installed Sound Blaster 16 (CT2720, YMF262 OPL3 + CT1741 DSP)");
        }
    }

    if config.midi == config::MidiDevice::Mt32 {
        if let Some(ref mt32_rom_dir) = config.mt32_roms {
            #[cfg(feature = "mt32")]
            {
                match bus.install_mt32(mt32_rom_dir) {
                    Ok(()) => info!("Loaded MT-32 sound module (munt)"),
                    Err(error) => warn!("MT-32 unavailable: {error}"),
                }
            }
            #[cfg(not(feature = "mt32"))]
            {
                let _ = mt32_rom_dir;
                warn!("MT-32 ROM path specified, but MT-32 support was not compiled in");
            }
        } else {
            warn!("MIDI device set to MT-32, but no MT-32 ROM directory specified (--mt32-roms)");
        }
    }

    if config.midi == config::MidiDevice::Sc55 {
        if let Some(ref sc55_rom_dir) = config.sc55_roms {
            #[cfg(feature = "sc55")]
            {
                match bus.install_sc55(sc55_rom_dir) {
                    Ok(()) => info!("Loaded Nuked-SC55 sound module"),
                    Err(error) => warn!("SC-55 unavailable: {error}"),
                }
            }
            #[cfg(not(feature = "sc55"))]
            {
                let _ = sc55_rom_dir;
                warn!("SC-55 ROM path specified, but SC-55 support was not compiled in");
            }
        } else {
            warn!("MIDI device set to SC-55, but no SC-55 ROM directory specified (--sc55-roms)");
        }
    }

    if let Some(ref printer_path) = config.printer {
        let file = File::options()
            .write(true)
            .open(printer_path)
            .with_context(|| {
                format!("Failed to open printer output: {}", printer_path.display())
            })?;
        info!("Printer attached: {}", printer_path.display());
        bus.attach_printer(file);
    }

    if let Some(ref hdd1_path) = config.hdd1 {
        let data = std::fs::read(hdd1_path)
            .with_context(|| format!("Failed to read HDD1 image from {}", hdd1_path.display()))?;
        let image = load_hdd_image(hdd1_path, &data)
            .with_context(|| format!("Failed to parse HDD1 image from {}", hdd1_path.display()))?;
        validate_hdd_for_machine(model, &image.geometry, "HDD1")?;
        info!(
            "Inserted HDD1: {}C/{}H/{}S ({}) from {}",
            image.geometry.cylinders,
            image.geometry.heads,
            image.geometry.sectors_per_track,
            image.format_name(),
            hdd1_path.display()
        );
        bus.insert_hdd(0, image, Some(hdd1_path.clone()));
    }

    if let Some(ref hdd2_path) = config.hdd2 {
        let data = std::fs::read(hdd2_path)
            .with_context(|| format!("Failed to read HDD2 image from {}", hdd2_path.display()))?;
        let image = load_hdd_image(hdd2_path, &data)
            .with_context(|| format!("Failed to parse HDD2 image from {}", hdd2_path.display()))?;
        validate_hdd_for_machine(model, &image.geometry, "HDD2")?;
        info!(
            "Inserted HDD2: {}C/{}H/{}S ({}) from {}",
            image.geometry.cylinders,
            image.geometry.heads,
            image.geometry.sectors_per_track,
            image.format_name(),
            hdd2_path.display()
        );
        bus.insert_hdd(1, image, Some(hdd2_path.clone()));
    }

    let machine: Box<dyn Machine> = match model.cpu_type() {
        common::CpuType::V30 => Box::new(machine::Machine::new(cpu::V30::new(), bus)),
        common::CpuType::I286 => Box::new(machine::Machine::new(cpu::I286::new(), bus)),
        common::CpuType::I386 => Box::new(machine::Machine::new(
            cpu::I386::<{ cpu::CPU_MODEL_386 }>::new(),
            bus,
        )),
        common::CpuType::I486DX => Box::new(machine::Machine::new(
            cpu::I386::<{ cpu::CPU_MODEL_486 }>::new(),
            bus,
        )),
    };

    Ok(machine)
}

fn validate_hdd_for_machine(
    model: MachineModel,
    geometry: &HddGeometry,
    label: &str,
) -> Result<()> {
    match model {
        MachineModel::PC9801VM | MachineModel::PC9801VX | MachineModel::PC9801RA => {
            ensure!(
                geometry.sasi_media_type().is_some(),
                "{label} is not compatible with {model} (SASI): \
                 geometry {}C/{}H/{}S with {}-byte sectors \
                 does not match any standard SASI drive type",
                geometry.cylinders,
                geometry.heads,
                geometry.sectors_per_track,
                geometry.sector_size,
            );
        }
        MachineModel::PC9821AS | MachineModel::PC9821AP => {
            ensure!(
                geometry.sector_size == 512,
                "{label} is not compatible with {model} (IDE): \
                 image has {}-byte sectors, but IDE requires 512-byte sectors",
                geometry.sector_size,
            );
        }
    }
    Ok(())
}

//! Lightweight logging facade with colored stdout output.

use std::{
    fmt,
    io::{BufWriter, Read, Stdout, Write},
    sync::{Mutex, OnceLock},
    time::Instant,
};

/// Log severity level.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Level {
    /// An error.
    Error = 1,
    /// A warning.
    Warn = 2,
    /// An informational message.
    Info = 3,
    /// A debug message.
    Debug = 4,
    /// A trace message.
    Trace = 5,
}

impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Level::Error => f.write_str("ERROR"),
            Level::Warn => f.write_str("WARN"),
            Level::Info => f.write_str("INFO"),
            Level::Debug => f.write_str("DEBUG"),
            Level::Trace => f.write_str("TRACE"),
        }
    }
}

/// A filter that matches log targets by prefix and assigns a maximum level.
pub struct PrefixFilter {
    prefix: &'static str,
    level: Level,
}

impl PrefixFilter {
    /// Creates a new [`PrefixFilter`].
    pub fn new(prefix: &'static str, level: Level) -> Self {
        Self { prefix, level }
    }

    fn matches(&self, target: &str, level: Level) -> Option<bool> {
        if target.starts_with(self.prefix) {
            Some(self.level >= level)
        } else {
            None
        }
    }
}

const RESET: &str = "\x1b[0m";
const RED: &str = "\x1b[31m";
const YELLOW: &str = "\x1b[33m";
const GREEN: &str = "\x1b[32m";
const BLUE: &str = "\x1b[34m";
const CYAN: &str = "\x1b[36m";

struct Logger {
    stdout: Mutex<BufWriter<Stdout>>,
    prefix_filter: Vec<PrefixFilter>,
    default_level: Level,
    start: Instant,
}

static LOGGER_INSTANCE: OnceLock<Logger> = OnceLock::new();

/// Initializes the global logger.
pub fn initialize_logger(default_level: Level, mut prefix_filter: Vec<PrefixFilter>) {
    let stdout = BufWriter::new(std::io::stdout());

    prefix_filter.sort_by_key(|f| f.prefix.len());
    prefix_filter.reverse();

    let logger = Logger {
        stdout: Mutex::new(stdout),
        prefix_filter,
        default_level,
        start: Instant::now(),
    };

    if LOGGER_INSTANCE.get().is_some() {
        panic!("Logger is already initialized");
    }

    LOGGER_INSTANCE.get_or_init(|| logger);
}

/// Returns `true` if the given level passes the default level filter.
pub fn log_enabled(level: Level) -> bool {
    let Some(logger) = LOGGER_INSTANCE.get() else {
        return false;
    };
    logger.default_level >= level
}

/// Returns `true` if the given level and target pass the configured filters.
pub fn log_enabled_for(level: Level, target: &str) -> bool {
    let Some(logger) = LOGGER_INSTANCE.get() else {
        return false;
    };

    for filter in logger.prefix_filter.iter() {
        if let Some(result) = filter.matches(target, level) {
            return result;
        }
    }

    logger.default_level >= level
}

#[doc(hidden)]
pub fn log_record(level: Level, target: &str, args: fmt::Arguments<'_>) {
    let Some(logger) = LOGGER_INSTANCE.get() else {
        return;
    };

    if !log_enabled_for(level, target) {
        return;
    }

    let duration_s = logger.start.elapsed().as_secs_f64();
    let mut stdout = logger.stdout.lock().expect("lock is poisoned");

    write!(&mut stdout, "[{duration_s:.2}][").expect("Can't write to stdout");

    match level {
        Level::Error => {
            write!(&mut stdout, "{RED}ERROR{RESET}").expect("Can't write to stdout");
        }
        Level::Warn => {
            write!(&mut stdout, "{YELLOW}WARN{RESET}").expect("Can't write to stdout");
        }
        Level::Info => {
            write!(&mut stdout, "{GREEN}INFO{RESET}").expect("Can't write to stdout");
        }
        Level::Debug => {
            write!(&mut stdout, "{BLUE}DEBUG{RESET}").expect("Can't write to stdout");
        }
        Level::Trace => {
            write!(&mut stdout, "{CYAN}TRACE{RESET}").expect("Can't write to stdout");
        }
    };

    writeln!(&mut stdout, "][{target}] {args:?}").expect("Can't write to stdout");
    stdout.flush().expect("Can't flush stdout");
}

/// Flushes the logger output.
pub fn flush() {
    let Some(logger) = LOGGER_INSTANCE.get() else {
        return;
    };

    logger
        .stdout
        .lock()
        .expect("lock is poisoned")
        .flush()
        .expect("Can't flush stdout");
}

/// Writes the content of the given reader to the buffered stdout.
pub fn write_to_stdout<R: Read>(reader: &mut R) {
    let logger = LOGGER_INSTANCE.get().expect("Logger was not initialized");
    let mut stdout = logger.stdout.lock().expect("lock is poisoned");
    std::io::copy(reader, &mut *stdout).expect("Could not write to stdout");
}

/// Logs a message at the error level.
#[macro_export]
macro_rules! error {
    (target: $target:expr, $($arg:tt)+) => {
        if $crate::log::log_enabled($crate::log::Level::Error) {
            $crate::log::log_record($crate::log::Level::Error, $target, format_args!($($arg)+));
        }
    };
    ($($arg:tt)+) => {
        if $crate::log::log_enabled($crate::log::Level::Error) {
            $crate::log::log_record($crate::log::Level::Error, module_path!(), format_args!($($arg)+));
        }
    };
}

/// Logs a message at the warn level.
#[macro_export]
macro_rules! warn {
    (target: $target:expr, $($arg:tt)+) => {
        if $crate::log::log_enabled($crate::log::Level::Warn) {
            $crate::log::log_record($crate::log::Level::Warn, $target, format_args!($($arg)+));
        }
    };
    ($($arg:tt)+) => {
        if $crate::log::log_enabled($crate::log::Level::Warn) {
            $crate::log::log_record($crate::log::Level::Warn, module_path!(), format_args!($($arg)+));
        }
    };
}

/// Logs a message at the info level.
#[macro_export]
macro_rules! info {
    (target: $target:expr, $($arg:tt)+) => {
        if $crate::log::log_enabled($crate::log::Level::Info) {
            $crate::log::log_record($crate::log::Level::Info, $target, format_args!($($arg)+));
        }
    };
    ($($arg:tt)+) => {
        if $crate::log::log_enabled($crate::log::Level::Info) {
            $crate::log::log_record($crate::log::Level::Info, module_path!(), format_args!($($arg)+));
        }
    };
}

/// Logs a message at the debug level.
#[macro_export]
macro_rules! debug {
    (target: $target:expr, $($arg:tt)+) => {
        if $crate::log::log_enabled($crate::log::Level::Debug) {
            $crate::log::log_record($crate::log::Level::Debug, $target, format_args!($($arg)+));
        }
    };
    ($($arg:tt)+) => {
        if $crate::log::log_enabled($crate::log::Level::Debug) {
            $crate::log::log_record($crate::log::Level::Debug, module_path!(), format_args!($($arg)+));
        }
    };
}

/// Logs a message at the trace level.
#[macro_export]
macro_rules! trace {
    (target: $target:expr, $($arg:tt)+) => {
        if $crate::log::log_enabled($crate::log::Level::Trace) {
            $crate::log::log_record($crate::log::Level::Trace, $target, format_args!($($arg)+));
        }
    };
    ($($arg:tt)+) => {
        if $crate::log::log_enabled($crate::log::Level::Trace) {
            $crate::log::log_record($crate::log::Level::Trace, module_path!(), format_args!($($arg)+));
        }
    };
}

/// Simple stderr logger, with level filter and color controllable by cli arguments.
use std::{
    ffi::OsStr,
    io::{self, Write},
    path::Path,
    str::FromStr,
};

use anyhow::{Error, Result};
use log::{Level, LevelFilter, Log, Metadata, Record};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, StandardStreamLock, WriteColor};

pub fn debug_file_name(path: &Path) -> &OsStr {
    let default = OsStr::new("<no file name?>");
    path.file_name().unwrap_or(default)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorHandling {
    Error,
    Warn,
    Ignore,
}

impl ErrorHandling {
    pub const NAMES: [&'static str; 3] = ["error", "warn", "ignore"];
}

impl FromStr for ErrorHandling {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "error" => Self::Error,
            "warn" => Self::Warn,
            "ignore" => Self::Ignore,
            _ => anyhow::bail!("Invalid error level: \"{}\"", s),
        })
    }
}

#[macro_export]
macro_rules! error_handling_handle {
    ($handling:expr, $fmt:expr, $($arg:tt)*) => {{
        match $handling {
            $crate::logging::ErrorHandling::Error => Err(::anyhow::format_err!($fmt, $($arg)*)),
            $crate::logging::ErrorHandling::Warn => {
                ::log::warn!($fmt, $($arg)*);
                Ok(())
            }
            $crate::logging::ErrorHandling::Ignore => {
                ::log::debug!("Ignoring: {}", ::std::format_args!($fmt, $($arg)*));
                Ok(())
            }
        }
    }}
}

fn write_record(writer: &mut StandardStreamLock<'_>, record: &Record<'_>) -> io::Result<()> {
    let level_color = match record.level() {
        Level::Trace => Color::Cyan,
        Level::Debug => Color::Blue,
        Level::Info => Color::Green,
        Level::Warn => Color::Yellow,
        Level::Error => Color::Red,
    };
    writer.set_color(
        ColorSpec::new()
            .set_fg(Some(level_color))
            .set_bold(record.level() == Level::Error),
    )?;
    write!(writer, "[{}]", record.level().as_str())?;
    writer.reset()?;
    writeln!(writer, " {}", record.args())
}

struct Logger {
    writer: StandardStream,
}

impl Log for Logger {
    fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &Record<'_>) {
        write_record(&mut self.writer.lock(), record).expect("failed to write to stderr");
    }

    fn flush(&self) {
        self.writer.lock().flush().expect("failed to flush stderr");
    }
}

pub fn setup(level: LevelFilter, color: ColorChoice) {
    let logger = Logger {
        writer: StandardStream::stderr(color),
    };

    log::set_max_level(level);
    if let Err(err) = log::set_boxed_logger(Box::new(logger)) {
        eprintln!("Failed to setup logger: {}", err);
    } else {
        log::trace!("Logger setup successfully");
    }
}

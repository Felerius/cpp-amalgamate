/// Simple stderr logger, with level filter and color controllable by cli arguments.
use std::{
    io::{self, Write},
    str::FromStr,
};

use anyhow::{Error, Result};
use log::{Level, LevelFilter, Log, Metadata, Record};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, StandardStreamLock, WriteColor};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorLevel {
    Error,
    Warn,
    Ignore,
}

#[macro_export]
macro_rules! error_level_enabled {
    ($error_level:expr) => {{
        match $error_level {
            $crate::logger::ErrorLevel::Warn => ::log::log_enabled!(::log::Level::Warn),
            $crate::logger::ErrorLevel::Error => true,
            $crate::logger::ErrorLevel::Ignore => ::log::log_enabled!(::log::Level::Debug),
        }
    }};
}

#[macro_export]
macro_rules! error_level_log {
    ($error_level:expr, $fmt:expr, $($arg:tt)*) => {{
        match $error_level {
            $crate::logger::ErrorLevel::Error => {
                // Let the error bubble up to main where it will be logged/printed
                ::anyhow::Result::Err(::anyhow::anyhow!($fmt, $($arg)*))
            }
            $crate::logger::ErrorLevel::Warn => {
                ::log::warn!($fmt, $($arg)*);
                ::anyhow::Result::Ok(())
            }
            $crate::logger::ErrorLevel::Ignore => {
                ::log::debug!(concat!("Ignoring: ", $fmt), $($arg)*);
                ::anyhow::Result::Ok(())
            }
        }
    }};
}

impl ErrorLevel {
    pub const NAMES: [&'static str; 3] = ["error", "warn", "ignore"];
}

impl FromStr for ErrorLevel {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "error" => Self::Error,
            "warn" => Self::Warn,
            "ignore" => Self::Ignore,
            _ => anyhow::bail!("invalid error level: \"{}\"", s),
        })
    }
}

struct Logger {
    writer: StandardStream,
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
    writer.set_color(&ColorSpec::new())?;
    writeln!(writer, " {}", record.args())
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

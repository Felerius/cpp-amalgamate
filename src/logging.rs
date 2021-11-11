/// Simple stderr logger, with level filter and color controllable by cli arguments.
use std::{ffi::OsStr, path::Path, str::FromStr};

use anyhow::{Error, Result};

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

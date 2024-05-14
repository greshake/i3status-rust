use unicode_segmentation::UnicodeSegmentation;

use std::fmt::Debug;
use std::time::Duration;

use super::parse::Arg;
use super::value::ValueInner as Value;
use super::FormatError;
use crate::config::SharedConfig;
use crate::errors::*;

// A helper macro for testing formatters
#[cfg(test)]
#[macro_export]
macro_rules! new_fmt {
    ($name:ident) => {{
        fmt!($name,)
    }};
    ($name:ident, $($key:ident : $value:tt),* $(,)?) => {
        new_formatter(stringify!($name), &[
            $( Arg { key: stringify!($key), val: stringify!($value) } ),*
        ])
    };
}

mod bar;
pub use bar::BarFormatter;
mod datetime;
pub use datetime::{DatetimeFormatter, DEFAULT_DATETIME_FORMATTER};
mod eng;
pub use eng::{EngFormatter, DEFAULT_NUMBER_FORMATTER};
mod flag;
pub use flag::{FlagFormatter, DEFAULT_FLAG_FORMATTER};
mod pango;
pub use pango::PangoStrFormatter;
mod str;
pub use str::{StrFormatter, DEFAULT_STRING_FORMATTER};

pub trait Formatter: Debug + Send + Sync {
    fn format(&self, val: &Value, config: &SharedConfig) -> Result<String, FormatError>;

    fn interval(&self) -> Option<Duration> {
        None
    }
}

pub fn new_formatter(name: &str, args: &[Arg]) -> Result<Box<dyn Formatter>> {
    match name {
        "bar" => Ok(Box::new(BarFormatter::from_args(args)?)),
        "datetime" => Ok(Box::new(DatetimeFormatter::from_args(args)?)),
        "eng" => Ok(Box::new(EngFormatter::from_args(args)?)),
        "pango-str" => Ok(Box::new(PangoStrFormatter::from_args(args)?)),
        "str" => Ok(Box::new(StrFormatter::from_args(args)?)),
        _ => Err(Error::new(format!("Unknown formatter: '{name}'"))),
    }
}

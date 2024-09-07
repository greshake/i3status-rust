use unicode_segmentation::UnicodeSegmentation;

use std::time::Duration;
use std::{borrow::Cow, fmt::Debug};

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
        new_fmt!($name,)
    }};
    ($name:ident, $($key:ident : $value:tt),* $(,)?) => {
        new_formatter(stringify!($name), &[
            $( Arg { key: stringify!($key), val: stringify!($value) } ),*
        ])
    };
}

mod bar;
pub use bar::BarFormatter;
mod tally;
pub use tally::TallyFormatter;
mod datetime;
pub use datetime::{DatetimeFormatter, DEFAULT_DATETIME_FORMATTER};
mod duration;
pub use duration::{DurationFormatter, DEFAULT_DURATION_FORMATTER};
mod eng;
pub use eng::{EngFormatter, DEFAULT_NUMBER_FORMATTER};
mod flag;
pub use flag::{FlagFormatter, DEFAULT_FLAG_FORMATTER};
mod pango;
pub use pango::PangoStrFormatter;
mod str;
pub use str::{StrFormatter, DEFAULT_STRING_FORMATTER};

type PadWith = Cow<'static, str>;

const DEFAULT_NUMBER_PAD_WITH: PadWith = Cow::Borrowed(" ");

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
        "dur" | "duration" => Ok(Box::new(DurationFormatter::from_args(args)?)),
        "eng" => Ok(Box::new(EngFormatter::from_args(args)?)),
        "pango-str" => Ok(Box::new(PangoStrFormatter::from_args(args)?)),
        "str" => Ok(Box::new(StrFormatter::from_args(args)?)),
        "tally" => Ok(Box::new(TallyFormatter::from_args(args)?)),
        _ => Err(Error::new(format!("Unknown formatter: '{name}'"))),
    }
}

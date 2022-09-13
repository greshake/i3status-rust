use std::fmt::Debug;
use std::iter::repeat;
use std::str::FromStr;
use std::time::{Duration, Instant};

use super::prefix::Prefix;
use super::unit::Unit;
use super::value::ValueInner as Value;
use crate::errors::*;
use crate::escape::CollectEscaped;

const DEFAULT_STR_MIN_WIDTH: usize = 0;
const DEFAULT_STR_MAX_WIDTH: Option<usize> = None;

const DEFAULT_STRROT_WIDTH: usize = 15;
const DEFAULT_STRROT_INTERVAL: f64 = 0.5;

const DEFAULT_BAR_WIDTH: usize = 5;
const DEFAULT_BAR_MAX_VAL: f64 = 100.0;

pub const DEFAULT_STRING_FORMATTER: StrFormatter = StrFormatter {
    min_width: DEFAULT_STR_MIN_WIDTH,
    max_width: DEFAULT_STR_MAX_WIDTH,
};

// TODO: split those defaults
pub const DEFAULT_NUMBER_FORMATTER: EngFormatter = EngFormatter(EngFixConfig {
    width: 2,
    unit: UnitConfig {
        unit: None,
        has_space: false,
        hidden: false,
    },
    prefix: PrefixConfig {
        prefix: None,
        has_space: false,
        hidden: false,
    },
});

pub const DEFAULT_FLAG_FORMATTER: FlagFormatter = FlagFormatter;

enum StrArgs {
    MinWidth,
    MaxWidth,
}

enum RotStrArgs {
    Width,
    Interval,
}

enum BarArgs {
    Width,
    MaxValue,
}

enum EngFixArgs {
    Width,
    Unit,
    Prefix,
}

pub trait Formatter: Debug + Send + Sync {
    fn format(&self, val: &Value) -> Result<String>;

    fn interval(&self) -> Option<Duration> {
        None
    }
}

pub fn new_formatter(name: &str, args: &[String]) -> Result<Box<dyn Formatter>> {
    match name {
        "str" => {
            let min_width: usize = match args.get(StrArgs::MinWidth as usize) {
                Some(v) => v.parse().error("Width must be a positive integer")?,
                None => DEFAULT_STR_MIN_WIDTH,
            };
            let max_width: Option<usize> =
                match args.get(StrArgs::MaxWidth as usize).map(|x| x.as_str()) {
                    Some("inf") => None,
                    Some(v) => Some(v.parse().error("Width must be a positive integer")?),
                    None => DEFAULT_STR_MAX_WIDTH,
                };
            if let Some(max_width) = max_width {
                if max_width < min_width {
                    return Err(Error::new(
                        "Max width must be greater of equal to min width",
                    ));
                }
            }
            Ok(Box::new(StrFormatter {
                min_width,
                max_width,
            }))
        }
        "rot-str" => {
            let width: usize = match args.get(RotStrArgs::Width as usize) {
                Some(v) => v.parse().error("Width must be a positive integer")?,
                None => DEFAULT_STRROT_WIDTH,
            };
            let interval: f64 = match args.get(RotStrArgs::Interval as usize) {
                Some(v) => v
                    .trim()
                    .parse()
                    .error("Interval must be a positive number")?,
                None => DEFAULT_STRROT_INTERVAL,
            };
            if interval < 0.1 {
                return Err(Error::new("Interval must be greater than 0.1"));
            }
            Ok(Box::new(RotStrFormatter {
                width,
                interval: (interval * 1000.0) as u64,
                init_time: Instant::now(),
            }))
        }
        "bar" => {
            let width: usize = match args.get(BarArgs::Width as usize) {
                Some(v) => v.parse().error("Width must be a positive integer")?,
                None => DEFAULT_BAR_WIDTH,
            };
            let max_value: f64 = match args.get(BarArgs::MaxValue as usize) {
                Some(v) => v.parse().error("Max value must be a number")?,
                None => DEFAULT_BAR_MAX_VAL,
            };
            Ok(Box::new(BarFormatter { width, max_value }))
        }
        "eng" => Ok(Box::new(EngFormatter(EngFixConfig::from_args(args)?))),
        "fix" => Ok(Box::new(FixFormatter(EngFixConfig::from_args(args)?))),
        _ => Err(Error::new(format!("Unknown formatter: '{}'", name))),
    }
}

#[derive(Debug)]
pub struct StrFormatter {
    min_width: usize,
    max_width: Option<usize>,
}

impl Formatter for StrFormatter {
    fn format(&self, val: &Value) -> Result<String> {
        match val {
            Value::Text(text) => {
                let width = text.chars().count();
                Ok(text
                    .chars()
                    .chain(repeat(' ').take(self.min_width.saturating_sub(width)))
                    .take(self.max_width.unwrap_or(usize::MAX))
                    .collect_pango())
            }
            Value::Icon(icon) => Ok(icon.clone()), // No escaping
            Value::Number { .. } => Err(Error::new_format(
                "A number cannot be formatted with 'str' formatter",
            )),
            Value::Flag => Err(Error::new_format(
                "A flag cannot be formatted with 'str' formatter",
            )),
        }
    }
}

#[derive(Debug)]
pub struct RotStrFormatter {
    width: usize,
    interval: u64,
    init_time: Instant,
}

impl Formatter for RotStrFormatter {
    fn format(&self, val: &Value) -> Result<String> {
        match val {
            Value::Text(text) => {
                let full_width = text.chars().count();
                if full_width <= self.width {
                    Ok(text
                        .chars()
                        .chain(repeat(' '))
                        .take(self.width)
                        .collect_pango())
                } else {
                    let full_width = full_width + 1; // Now we include '|' at the end
                    let step = (self.init_time.elapsed().as_millis() as u64 / self.interval)
                        as usize
                        % full_width;
                    let w1 = self.width.min(full_width - step);
                    Ok(text
                        .chars()
                        .chain(Some('|'))
                        .skip(step)
                        .take(w1)
                        .chain(text.chars())
                        .take(self.width)
                        .collect_pango())
                }
            }
            Value::Icon(_) => Err(Error::new_format(
                "An icon cannot be formatted with 'rot-str' formatter",
            )),
            Value::Number { .. } => Err(Error::new_format(
                "A number cannot be formatted with 'rot-str' formatter",
            )),
            Value::Flag => Err(Error::new_format(
                "A flag cannot be formatted with 'rot-str' formatter",
            )),
        }
    }

    fn interval(&self) -> Option<Duration> {
        Some(Duration::from_millis(self.interval))
    }
}

#[derive(Debug)]
pub struct BarFormatter {
    width: usize,
    max_value: f64,
}

const VERTICAL_BAR_CHARS: [char; 9] = [
    ' ', '\u{258f}', '\u{258e}', '\u{258d}', '\u{258c}', '\u{258b}', '\u{258a}', '\u{2589}',
    '\u{2588}',
];

impl Formatter for BarFormatter {
    fn format(&self, val: &Value) -> Result<String> {
        match val {
            Value::Number { mut val, .. } => {
                val = (val / self.max_value).clamp(0., 1.);
                let chars_to_fill = val * self.width as f64;
                Ok((0..self.width)
                    .map(|i| {
                        VERTICAL_BAR_CHARS[((chars_to_fill - i as f64).clamp(0., 1.) * 8.) as usize]
                    })
                    .collect())
            }
            Value::Text(_) => Err(Error::new_format(
                "Text cannot be formatted with 'bar' formatter",
            )),
            Value::Icon(_) => Err(Error::new_format(
                "An icon cannot be formatted with 'bar' formatter",
            )),
            Value::Flag => Err(Error::new_format(
                "A flag cannot be formatted with 'bar' formatter",
            )),
        }
    }
}

#[derive(Debug, Default)]
struct PrefixConfig {
    pub prefix: Option<(Prefix, bool)>,
    pub has_space: bool,
    pub hidden: bool,
}

impl FromStr for PrefixConfig {
    type Err = Error;

    fn from_str(mut s: &str) -> Result<Self> {
        let has_space = if s.starts_with(' ') {
            s = &s[1..];
            true
        } else {
            false
        };

        let hidden = if s.starts_with('_') {
            s = &s[1..];
            true
        } else {
            false
        };

        let forced = if s.starts_with('!') {
            s = &s[1..];
            true
        } else {
            false
        };

        let prefix = if s == "auto" {
            None
        } else {
            Some((s.parse()?, forced))
        };

        Ok(Self {
            prefix,
            has_space,
            hidden,
        })
    }
}

#[derive(Debug, Default)]
struct UnitConfig {
    pub unit: Option<Unit>,
    pub has_space: bool,
    pub hidden: bool,
}

impl FromStr for UnitConfig {
    type Err = Error;

    fn from_str(mut s: &str) -> Result<Self> {
        let has_space = if s.starts_with(' ') {
            s = &s[1..];
            true
        } else {
            false
        };

        let hidden = if s.starts_with('_') {
            s = &s[1..];
            true
        } else {
            false
        };

        Ok(Self {
            unit: if s == "auto" { None } else { Some(s.parse()?) },
            has_space,
            hidden,
        })
    }
}

#[derive(Debug)]
struct EngFixConfig {
    width: usize,
    unit: UnitConfig,
    prefix: PrefixConfig,
}

impl EngFixConfig {
    fn from_args(args: &[String]) -> Result<Self> {
        Ok(Self {
            width: args
                .get(EngFixArgs::Width as usize)
                .map(|x| x.parse::<usize>())
                .transpose()
                .error("Width must be a positive integer")?
                .unwrap_or(3),
            unit: args
                .get(EngFixArgs::Unit as usize)
                .map(|x| x.parse::<UnitConfig>())
                .transpose()?
                .unwrap_or_default(),
            prefix: args
                .get(EngFixArgs::Prefix as usize)
                .map(|x| x.parse::<PrefixConfig>())
                .transpose()?
                .unwrap_or_default(),
        })
    }
}

#[derive(Debug)]
pub struct EngFormatter(EngFixConfig);

impl Formatter for EngFormatter {
    fn format(&self, val: &Value) -> Result<String> {
        match val {
            Value::Number { mut val, mut unit } => {
                if let Some(new_unit) = self.0.unit.unit {
                    val = unit.convert(val, new_unit)?;
                    unit = new_unit;
                }

                let (min_prefix, max_prefix) = match self.0.prefix.prefix {
                    Some((prefix, true)) => (prefix, prefix),
                    Some((prefix, false)) => (prefix, Prefix::max_available()),
                    None => (Prefix::min_available(), Prefix::max_available()),
                };

                let prefix = unit
                    .clamp_prefix(if min_prefix.is_binary() {
                        Prefix::eng_binary(val)
                    } else {
                        Prefix::eng(val)
                    })
                    .clamp(min_prefix, max_prefix);
                val = prefix.apply(val);

                let mut digits = (val.max(1.).log10().floor() + 1.0) as isize;
                if val < 0. {
                    digits += 1;
                }

                let mut retval = match self.0.width as isize - digits {
                    isize::MIN..=0 => format!("{}", val.floor()),
                    1 => format!(" {}", val.floor() as i64),
                    rest => format!("{:.*}", rest as usize - 1, val),
                };

                let display_prefix = !self.0.prefix.hidden
                    && prefix != Prefix::One
                    && prefix != Prefix::OneButBinary;
                let display_unit = !self.0.unit.hidden && unit != Unit::None;

                if display_prefix {
                    if self.0.prefix.has_space {
                        retval.push(' ');
                    }
                    retval.push_str(&prefix.to_string());
                }
                if display_unit {
                    if self.0.unit.has_space || (self.0.prefix.has_space && !display_prefix) {
                        retval.push(' ');
                    }
                    retval.push_str(&unit.to_string());
                }

                Ok(retval)
            }
            Value::Text(_) => Err(Error::new_format(
                "Text cannot be formatted with 'eng' formatter",
            )),
            Value::Icon(_) => Err(Error::new_format(
                "An icon cannot be formatted with 'eng' formatter",
            )),
            Value::Flag => Err(Error::new_format(
                "A flag cannot be formatted with 'eng' formatter",
            )),
        }
    }
}

#[derive(Debug)]
pub struct FixFormatter(EngFixConfig);

impl Formatter for FixFormatter {
    fn format(&self, val: &Value) -> Result<String> {
        match val {
            Value::Number {
                ..
                // mut val,
                // unit,
                // icon,
            } => Err(Error::new_format("'fix' formatter is not implemented yet")),
            Value::Text(_) => Err(Error::new_format(
                "Text cannot be formatted with 'fix' formatter",
            )),
            Value::Icon(_) => Err(Error::new_format(
                "An icon cannot be formatted with 'fix' formatter",
            )),
            Value::Flag => Err(Error::new_format(
                "A flag cannot be formatted with 'fix' formatter",
            )),
        }
    }
}

#[derive(Debug)]
pub struct FlagFormatter;

impl Formatter for FlagFormatter {
    fn format(&self, val: &Value) -> Result<String> {
        match val {
            Value::Number { .. } | Value::Text(_) | Value::Icon(_) => unreachable!(),
            Value::Flag => Ok(String::new()),
        }
    }
}

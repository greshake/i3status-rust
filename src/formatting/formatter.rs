use chrono::format::{Item, StrftimeItems};
use chrono::{Local, Locale};
use once_cell::sync::Lazy;

use std::fmt::Debug;
use std::iter::repeat;
use std::time::{Duration, Instant};

use super::parse::Arg;
use super::prefix::Prefix;
use super::unit::Unit;
use super::value::ValueInner as Value;
use crate::errors::*;
use crate::escape::CollectEscaped;

const DEFAULT_STR_MIN_WIDTH: usize = 0;
const DEFAULT_STR_MAX_WIDTH: usize = usize::MAX;
const DEFAULT_STR_ROT_INTERVAL: Option<f64> = None;

const DEFAULT_BAR_WIDTH: usize = 5;
const DEFAULT_BAR_MAX_VAL: f64 = 100.0;

const DEFAULT_NUMBER_WIDTH: usize = 2;

const DEFAULT_DATETIME_FORMAT: &str = "%a %d/%m %R";

pub const DEFAULT_STRING_FORMATTER: StrFormatter = StrFormatter {
    min_width: DEFAULT_STR_MIN_WIDTH,
    max_width: DEFAULT_STR_MAX_WIDTH,
    rot_interval_ms: None,
    init_time: None,
};

// TODO: split those defaults
pub const DEFAULT_NUMBER_FORMATTER: EngFormatter = EngFormatter(EngFixConfig {
    width: DEFAULT_NUMBER_WIDTH,
    unit: None,
    unit_has_space: false,
    unit_hidden: false,
    prefix: None,
    prefix_has_space: false,
    prefix_hidden: false,
    prefix_forced: false,
});

pub static DEFAULT_DATETIME_FORMATTER: Lazy<DatetimeFormatter> =
    Lazy::new(|| DatetimeFormatter::new(DEFAULT_DATETIME_FORMAT, None).unwrap());

pub const DEFAULT_FLAG_FORMATTER: FlagFormatter = FlagFormatter;

pub trait Formatter: Debug + Send + Sync {
    fn format(&self, val: &Value) -> Result<String>;

    fn interval(&self) -> Option<Duration> {
        None
    }
}

pub fn new_formatter(name: &str, args: &[Arg]) -> Result<Box<dyn Formatter>> {
    match name {
        "str" => {
            let mut min_width = DEFAULT_STR_MIN_WIDTH;
            let mut max_width = DEFAULT_STR_MAX_WIDTH;
            let mut rot_interval = DEFAULT_STR_ROT_INTERVAL;
            for arg in args {
                match arg.key {
                    "min_width" | "min_w" => {
                        min_width = arg.val.parse().error("Width must be a positive integer")?;
                    }
                    "max_width" | "max_w" => {
                        max_width = arg.val.parse().error("Width must be a positive integer")?;
                    }
                    "width" | "w" => {
                        min_width = arg.val.parse().error("Width must be a positive integer")?;
                        max_width = min_width;
                    }
                    "rot_interval" => {
                        rot_interval = Some(
                            arg.val
                                .parse()
                                .error("Interval must be a positive number")?,
                        );
                    }
                    other => {
                        return Err(Error::new(format!("Unknown argument for 'str': '{other}'")));
                    }
                }
            }
            if max_width < min_width {
                return Err(Error::new(
                    "Max width must be greater of equal to min width",
                ));
            }
            if let Some(rot_interval) = rot_interval {
                if rot_interval < 0.1 {
                    return Err(Error::new("Interval must be greater than 0.1"));
                }
            }
            Ok(Box::new(StrFormatter {
                min_width,
                max_width,
                rot_interval_ms: rot_interval.map(|x| (x * 1e3) as u64),
                init_time: Some(Instant::now()),
            }))
        }
        "pango-str" => {
            #[allow(clippy::never_loop)]
            for arg in args {
                return Err(Error::new(format!(
                    "Unknown argument for 'pango-str': '{}'",
                    arg.key
                )));
            }
            Ok(Box::new(PangoStrFormatter))
        }
        "bar" => {
            let mut width = DEFAULT_BAR_WIDTH;
            let mut max_value = DEFAULT_BAR_MAX_VAL;
            for arg in args {
                match arg.key {
                    "width" | "w" => {
                        width = arg.val.parse().error("Width must be a positive integer")?;
                    }
                    "max_value" => {
                        max_value = arg.val.parse().error("Max value must be a number")?;
                    }
                    other => {
                        return Err(Error::new(format!("Unknown argument for 'bar': '{other}'")));
                    }
                }
            }
            Ok(Box::new(BarFormatter { width, max_value }))
        }
        "eng" => Ok(Box::new(EngFormatter(EngFixConfig::from_args(args)?))),
        "fix" => Ok(Box::new(FixFormatter(EngFixConfig::from_args(args)?))),
        "datetime" => {
            let mut format = None;
            let mut locale = None;
            for arg in args {
                match arg.key {
                    "format" | "f" => {
                        format = Some(arg.val);
                    }
                    "locale" | "l" => {
                        locale = Some(arg.val);
                    }
                    other => {
                        return Err(Error::new(format!(
                            "Unknown argument for 'datetime': '{other}'"
                        )));
                    }
                }
            }

            Ok(Box::new(DatetimeFormatter::new(
                format.unwrap_or(DEFAULT_DATETIME_FORMAT),
                locale,
            )?))
        }
        _ => Err(Error::new(format!("Unknown formatter: '{name}'"))),
    }
}

#[derive(Debug)]
pub struct StrFormatter {
    min_width: usize,
    max_width: usize,
    rot_interval_ms: Option<u64>,
    init_time: Option<Instant>,
}

impl Formatter for StrFormatter {
    fn format(&self, val: &Value) -> Result<String> {
        match val {
            Value::Text(text) => {
                let width = text.chars().count();
                Ok(match (self.rot_interval_ms, self.init_time) {
                    (Some(rot_interval_ms), Some(init_time)) if width > self.max_width => {
                        let width = width + 1; // Now we include '|' at the end
                        let step = (init_time.elapsed().as_millis() as u64 / rot_interval_ms)
                            as usize
                            % width;
                        let w1 = self.max_width.min(width - step);
                        text.chars()
                            .chain(Some('|'))
                            .skip(step)
                            .take(w1)
                            .chain(text.chars())
                            .take(self.max_width)
                            .collect_pango_escaped()
                    }
                    _ => text
                        .chars()
                        .chain(repeat(' ').take(self.min_width.saturating_sub(width)))
                        .take(self.max_width)
                        .collect_pango_escaped(),
                })
            }
            Value::Icon(icon) => Ok(icon.clone()), // No escaping
            other => Err(Error::new_format(format!(
                "{} cannot be formatted with 'str' formatter",
                other.type_name(),
            ))),
        }
    }

    fn interval(&self) -> Option<Duration> {
        self.rot_interval_ms.map(Duration::from_millis)
    }
}

#[derive(Debug)]
pub struct PangoStrFormatter;

impl Formatter for PangoStrFormatter {
    fn format(&self, val: &Value) -> Result<String> {
        match val {
            Value::Text(x) | Value::Icon(x) => Ok(x.clone()), // No escaping
            other => Err(Error::new_format(format!(
                "{} cannot be formatted with 'str' formatter",
                other.type_name(),
            ))),
        }
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
            other => Err(Error::new_format(format!(
                "{} cannot be formatted with 'bar' formatter",
                other.type_name(),
            ))),
        }
    }
}

#[derive(Debug)]
struct EngFixConfig {
    width: usize,
    unit: Option<Unit>,
    unit_has_space: bool,
    unit_hidden: bool,
    prefix: Option<Prefix>,
    prefix_has_space: bool,
    prefix_hidden: bool,
    prefix_forced: bool,
}

impl EngFixConfig {
    fn from_args(args: &[Arg]) -> Result<Self> {
        let mut width = DEFAULT_NUMBER_WIDTH;
        let mut unit = None;
        let mut unit_has_space = false;
        let mut unit_hidden = false;
        let mut prefix = None;
        let mut prefix_has_space = false;
        let mut prefix_hidden = false;
        let mut prefix_forced = false;

        for arg in args {
            match arg.key {
                "width" | "w" => {
                    width = arg.val.parse().error("Width must be a positive integer")?;
                }
                "unit" | "u" => {
                    unit = Some(arg.val.parse()?);
                }
                "hide_unit" => {
                    unit_hidden = arg.val.parse().error("hide_unit must be true or false")?;
                }
                "unit_space" => {
                    unit_has_space = arg.val.parse().error("unit_space must be true or false")?;
                }
                "prefix" | "p" => {
                    prefix = Some(arg.val.parse()?);
                }
                "hide_prefix" => {
                    prefix_hidden = arg.val.parse().error("hide_prefix must be true or false")?;
                }
                "prefix_space" => {
                    prefix_has_space = arg
                        .val
                        .parse()
                        .error("prefix_space must be true or false")?;
                }
                "force_prefix" => {
                    prefix_forced = arg
                        .val
                        .parse()
                        .error("force_prefix must be true or false")?;
                }
                other => {
                    return Err(Error::new(format!(
                        "Unknown argument for 'fix'/'eng': '{other}'"
                    )));
                }
            }
        }

        Ok(Self {
            width,
            unit,
            unit_has_space,
            unit_hidden,
            prefix,
            prefix_has_space,
            prefix_hidden,
            prefix_forced,
        })
    }
}

#[derive(Debug)]
pub struct EngFormatter(EngFixConfig);

impl Formatter for EngFormatter {
    fn format(&self, val: &Value) -> Result<String> {
        match val {
            Value::Number { mut val, mut unit } => {
                if let Some(new_unit) = self.0.unit {
                    val = unit.convert(val, new_unit)?;
                    unit = new_unit;
                }

                let (min_prefix, max_prefix) = match (self.0.prefix, self.0.prefix_forced) {
                    (Some(prefix), true) => (prefix, prefix),
                    (Some(prefix), false) => (prefix, Prefix::max_available()),
                    (None, _) => (Prefix::min_available(), Prefix::max_available()),
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

                let display_prefix = !self.0.prefix_hidden
                    && prefix != Prefix::One
                    && prefix != Prefix::OneButBinary;
                let display_unit = !self.0.unit_hidden && unit != Unit::None;

                if display_prefix {
                    if self.0.prefix_has_space {
                        retval.push(' ');
                    }
                    retval.push_str(&prefix.to_string());
                }
                if display_unit {
                    if self.0.unit_has_space || (self.0.prefix_has_space && !display_prefix) {
                        retval.push(' ');
                    }
                    retval.push_str(&unit.to_string());
                }

                Ok(retval)
            }
            other => Err(Error::new_format(format!(
                "{} cannot be formatted with 'eng' formatter",
                other.type_name(),
            ))),
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
            other => Err(Error::new_format(format!(
                "{} cannot be formatted with 'fix' formatter",
                other.type_name(),
            )))
        }
    }
}

#[derive(Debug)]
pub struct DatetimeFormatter {
    items: Vec<Item<'static>>,
    locale: Option<Locale>,
}

fn make_static_item(item: Item<'_>) -> Item<'static> {
    match item {
        Item::Literal(str) => Item::OwnedLiteral(str.into()),
        Item::OwnedLiteral(boxed) => Item::OwnedLiteral(boxed),
        Item::Space(str) => Item::OwnedSpace(str.into()),
        Item::OwnedSpace(boxed) => Item::OwnedSpace(boxed),
        Item::Numeric(numeric, pad) => Item::Numeric(numeric, pad),
        Item::Fixed(fixed) => Item::Fixed(fixed),
        Item::Error => Item::Error,
    }
}

impl DatetimeFormatter {
    fn new(format: &str, locale: Option<&str>) -> Result<Self> {
        let (items, locale) = match locale {
            Some(locale) => {
                let locale = locale.try_into().ok().error("invalid locale")?;
                (StrftimeItems::new_with_locale(format, locale), Some(locale))
            }
            None => (StrftimeItems::new(format), None),
        };

        Ok(Self {
            items: items.map(make_static_item).collect(),
            locale,
        })
    }
}

impl Formatter for DatetimeFormatter {
    fn format(&self, val: &Value) -> Result<String> {
        match val {
            Value::Datetime(datetime, timezone) => Ok(match self.locale {
                Some(locale) => match timezone {
                    Some(tz) => datetime
                        .with_timezone(tz)
                        .format_localized_with_items(self.items.iter(), locale),
                    None => datetime
                        .with_timezone(&Local)
                        .format_localized_with_items(self.items.iter(), locale),
                },
                None => match timezone {
                    Some(tz) => datetime
                        .with_timezone(tz)
                        .format_with_items(self.items.iter()),
                    None => datetime
                        .with_timezone(&Local)
                        .format_with_items(self.items.iter()),
                },
            }
            .to_string()),
            other => Err(Error::new_format(format!(
                "{} cannot be formatted with 'datetime' formatter",
                other.type_name(),
            ))),
        }
    }
}

#[derive(Debug)]
pub struct FlagFormatter;

impl Formatter for FlagFormatter {
    fn format(&self, val: &Value) -> Result<String> {
        match val {
            Value::Flag => Ok(String::new()),
            _ => {
                unreachable!()
            }
        }
    }
}

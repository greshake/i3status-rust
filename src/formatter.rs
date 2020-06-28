use std::borrow::{Borrow, Cow};
use std::cmp::min;
use std::collections::HashMap;
use std::fmt;
use std::hash::Hash;

use lazy_static::lazy_static;
use regex::Regex;

use crate::errors::*;
use crate::util::Prefix;

pub trait Format {
    fn format(&self, formatter: &VarFormatter) -> Option<String>;
}

#[derive(Clone, Debug)]
pub struct FormatTemplate {
    inner: Vec<FormatAtom>,
}

#[derive(Clone, Debug)]
enum FormatAtom {
    Str(String),
    Var {
        name: String,
        formatter: VarFormatter,
    },
}

#[derive(Clone, Debug)]
pub enum VarFormatter {
    Regular,
    Bytes {
        unit: BytesUnit,
        digits: usize,
        min_prefix: Prefix,
    },
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum BytesUnit {
    Bits,
    Bytes,
}

// --
// -- Formatter implementation
// --

impl FormatTemplate {
    pub fn from_string(s: &str, icons: &HashMap<String, String>) -> Result<Self> {
        lazy_static! {
            static ref RE: Regex = {
                let match_var = r"\{(?P<var>[^}]*)\}";
                let match_icon = r"<(?P<icon>[a-zA-Z0-9_]+)>";
                let match_text = r"(?P<text>[^{<]+)";
                Regex::new(&format!(r"{}|{}|{}", match_var, match_icon, match_text))
                    .expect("invalid format regex")
            };
        }

        let inner = RE
            .captures_iter(&s)
            .map(|re_match| {
                Ok(
                    match (
                        re_match.name("text"),
                        re_match.name("var"),
                        re_match.name("icon"),
                    ) {
                        (Some(text), None, None) => FormatAtom::Str(text.as_str().to_string()),
                        (None, Some(formatter), None) => {
                            FormatAtom::from_format_param(formatter.as_str())?
                        }
                        (None, None, Some(icon)) => FormatAtom::Str(
                            icons
                                .get(icon.as_str())
                                .ok_or_else(|| {
                                    Error::InternalError(
                                        "formatter".to_string(),
                                        format!("unknown icon: '{}'", icon.as_str()),
                                        None,
                                    )
                                })?
                                .trim()
                                .to_string(),
                        ),
                        _ => unreachable!("invalid regex: should produce exactly a variant"),
                    },
                )
            })
            .collect::<Result<_>>()?;

        dbg!(&inner);

        Ok(Self { inner })
    }

    pub fn render<K, T>(&self, vars: &HashMap<K, T>) -> Result<String>
    where
        K: Eq + Hash + Borrow<str>,
        T: Format,
    {
        self.inner
            .iter()
            .map(|atom| {
                Ok(match atom {
                    FormatAtom::Str(text) => Cow::from(text),
                    FormatAtom::Var { name, formatter } => Cow::from(
                        vars.get(name)
                            .internal_error("util", &format!("unknown variable: {}", name))?
                            .format(formatter)
                            .ok_or(Error::InvalidFormatter {
                                var: name.to_string(),
                                formatter: formatter.clone(),
                            })?,
                    ),
                })
            })
            .try_fold(String::new(), |acc, atom: Result<_>| Ok(acc + &atom?))
    }
}

impl FormatAtom {
    fn parse_formatter_params(params: &str) -> HashMap<&str, &str> {
        lazy_static! {
            static ref RE: Regex = Regex::new(r"(?P<key>[^=,]+)=(?P<val>[^=,]+)")
                .expect("invalid formatter params regex");
        }

        RE.captures_iter(params)
            .map(|re_match| {
                let key = re_match.name("key").expect("invalid regex: missing `key`");
                let val = re_match.name("val").expect("invalid regex: missing `val`");
                (key.as_str(), val.as_str())
            })
            .collect()
    }

    fn from_format_param(param: &str) -> Result<Self> {
        lazy_static! {
            static ref RE: Regex = {
                let name = r"(?P<name>[a-zA-Z0-9_-]+)";
                let formatter_id = r"(?P<formatter_id>B)";
                let formatter_param = r"[^\[\]=]+=[^\[\]=]+";

                let formatter_params = format!(
                    r"(\[(?P<formatter_params>({param}(,{param})*)?)\])",
                    param = formatter_param,
                );

                let formatter = format!(
                    r"{name}?{params}?",
                    name = formatter_id,
                    params = formatter_params,
                );

                Regex::new(&format!(
                    r"{name}(:{formatter})?",
                    name = name,
                    formatter = formatter
                ))
                .expect("invalid formater regex")
            };
        }

        let groups = RE
            .captures(param)
            .internal_error("util", &format!("invalid format parameter: {}", param))?;

        let name = groups
            .name("name")
            .expect("invalid Regex for FormatAtom: name not found")
            .as_str()
            .to_string();

        let params = groups
            .name("formatter_params")
            .map(|s| FormatAtom::parse_formatter_params(s.as_str()))
            .unwrap_or_default();

        let formatter = groups
            .name("formatter_id")
            .map(|s| match s.as_str() {
                "B" => VarFormatter::bytes_with(params),
                _ => unreachable!("invalid Regex for FormatAtom: unrecognised formatter id"),
            })
            .unwrap_or(Ok(VarFormatter::Regular))?;

        Ok(Self::Var { name, formatter })
    }
}

impl VarFormatter {
    fn bytes_with(params: HashMap<&str, &str>) -> Result<Self> {
        let unit = match params.get("unit") {
            Some(&"B") | None => BytesUnit::Bytes,
            Some(&"b") => BytesUnit::Bits,
            Some(other_unit) => {
                return Err(Error::InternalError(
                    "formatter".to_string(),
                    format!("invalid bytes unit: {}", other_unit),
                    None,
                ))
            }
        };

        let digits = params
            .get("digits")
            .map(|s| {
                s.parse()
                    .internal_error("formatter", &format!("invalid digits parameter: '{}'", s))
            })
            .unwrap_or(Ok(2))?;

        let min_prefix = params
            .get("min_prefix")
            .map(|s| {
                Prefix::from_str(s).ok_or_else(|| {
                    Error::InternalError(
                        "formatter".to_string(),
                        format!("invalid prefix '{}'", s),
                        None,
                    )
                })
            })
            .unwrap_or(Ok(Prefix::None))?;

        Ok(Self::Bytes {
            unit,
            digits,
            min_prefix,
        })
    }
}

// --
// -- Format implementation for standart types
// --

impl<T: Format + ?Sized> Format for &T {
    fn format(&self, formatter: &VarFormatter) -> Option<String> {
        (*self).format(formatter)
    }
}

impl Format for &str {
    fn format(&self, formatter: &VarFormatter) -> Option<String> {
        match formatter {
            VarFormatter::Regular => Some(self.to_string()),
            _ => None,
        }
    }
}

impl Format for String {
    fn format(&self, formatter: &VarFormatter) -> Option<String> {
        self.as_str().format(formatter)
    }
}

macro_rules! impl_format_for_numeric {
    ($t:ty) => {
        impl Format for $t {
            fn format(&self, formatter: &VarFormatter) -> Option<String> {
                match formatter {
                    VarFormatter::Regular => Some(format!("{}", self)),
                    _ => None,
                }
            }
        }
    };
}

impl_format_for_numeric!(f64);
impl_format_for_numeric!(f32);
impl_format_for_numeric!(usize);
impl_format_for_numeric!(isize);
impl_format_for_numeric!(u64);
impl_format_for_numeric!(u32);
impl_format_for_numeric!(u16);
impl_format_for_numeric!(u8);
impl_format_for_numeric!(i64);
impl_format_for_numeric!(i32);
impl_format_for_numeric!(i16);
impl_format_for_numeric!(i8);

// --
// -- Structures used for custom formatters
// --

impl fmt::Display for Prefix {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// --
// -- Custom types for formatter
// --

pub struct Bytes(pub f64);

impl Format for Bytes {
    fn format(&self, formatter: &VarFormatter) -> Option<String> {
        let Self(mut val) = self;

        match formatter {
            VarFormatter::Regular => val.format(formatter),
            VarFormatter::Bytes {
                unit,
                digits,
                min_prefix: mut prefix,
            } => {
                if *unit == BytesUnit::Bits {
                    val *= 8.
                }

                while val / prefix.factor() >= 1000. && prefix.next().is_some() {
                    prefix = prefix.next().unwrap()
                }

                Some(format!(
                    "{}{}",
                    format_with_digits(val / prefix.factor(), *digits),
                    prefix
                ))
            }
        }
    }
}

/// --
/// -- Formatting utilities
/// --

fn format_with_digits(value: f64, digits: usize) -> String {
    let integer = {
        if value < 1. {
            0
        } else {
            1 + value.log(10.) as usize
        }
    };

    let decimal = digits - min(digits, integer);
    format!("{:.*}", decimal, value)
}

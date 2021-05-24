use std::str::FromStr;

use super::prefix::Prefix;
use super::unit::Unit;
use crate::errors::*;

const DELIMETERS: &[char] = &[':', '^', ';', '*', '#'];
const MIN_WIDTH_TOKEN: char = DELIMETERS[0];
const MAX_WIDTH_TOKEN: char = DELIMETERS[1];
const MIN_PREFIX_TOKEN: char = DELIMETERS[2];
const UNIT_TOKEN: char = DELIMETERS[3];
const BAR_MAX_VAL_TOKEN: char = DELIMETERS[4];

#[derive(Debug, Clone, PartialEq)]
pub struct Placeholder {
    pub name: String,
    pub min_width: MinWidthConfig,
    pub unit: UnitConfig,
    pub min_prefix: MinPrefixConfig,
    pub max_width: Option<usize>,
    pub bar_max_value: Option<f64>,
}

pub(super) fn unexpected_token<T>(token: char) -> Result<T> {
    Err(InternalError(
        "format parser".to_string(),
        format!("unexpected token '{}'", token),
        None,
    ))
}

impl FromStr for Placeholder {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        // A handy macro for parsing placeholders configuration
        macro_rules! parse {
            ($delim:expr) => {
                match s.split_once($delim) {
                    None => "",
                    Some((_, min_width)) => match min_width.split_once(DELIMETERS) {
                        None => min_width,
                        Some((min_width, _)) => min_width,
                    },
                }
            };
        }

        let name = s.split_once(DELIMETERS).unwrap_or((s, "")).0;
        let min_width = parse!(MIN_WIDTH_TOKEN);
        let max_width = parse!(MAX_WIDTH_TOKEN);
        let min_prefix = parse!(MIN_PREFIX_TOKEN);
        let unit = parse!(UNIT_TOKEN);
        let bar_max_value = parse!(BAR_MAX_VAL_TOKEN);

        // Parse max_width
        let max_width = if max_width.is_empty() {
            None
        } else {
            Some(max_width.parse().internal_error(
                "format parser",
                &format!("failed to parse max_width '{}'", max_width),
            )?)
        };
        // Parse bar_max_value
        let bar_max_value = if bar_max_value.is_empty() {
            None
        } else {
            Some(bar_max_value.parse().internal_error(
                "format parser",
                &format!("failed to parse bar_max_value '{}'", bar_max_value),
            )?)
        };

        Ok(Self {
            name: name.to_string(),
            min_width: min_width.parse()?,
            unit: unit.parse()?,
            min_prefix: min_prefix.parse()?,
            max_width,
            bar_max_value,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MinWidthConfig {
    pub min_width: Option<usize>,
    pub pad_with: char,
}

impl FromStr for MinWidthConfig {
    type Err = Error;

    fn from_str(mut s: &str) -> Result<Self> {
        let pad_with_zero = s.starts_with('0');
        if pad_with_zero {
            s = &s[1..];
        }

        Ok(MinWidthConfig {
            min_width: if s.is_empty() {
                None
            } else {
                Some(s.parse().internal_error(
                    "format parser",
                    &format!("failed to parse min_width '{}'", s),
                )?)
            },
            pad_with: if pad_with_zero { '0' } else { ' ' },
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UnitConfig {
    pub unit: Option<Unit>,
    pub hidden: bool,
}

impl FromStr for UnitConfig {
    type Err = Error;

    fn from_str(mut s: &str) -> Result<Self> {
        let hidden = s.starts_with('_');
        if hidden {
            s = &s[1..];
        }
        Ok(UnitConfig {
            unit: if s.is_empty() { None } else { Some(s.parse()?) },
            hidden,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MinPrefixConfig {
    pub value: Option<Prefix>,
    pub space: bool,
    pub hidden: bool,
}

impl FromStr for MinPrefixConfig {
    type Err = Error;

    fn from_str(mut s: &str) -> Result<Self> {
        let space = s.starts_with(' ');
        if space {
            s = &s[1..];
        }
        let hidden = s.starts_with('_');
        if hidden {
            s = &s[1..];
        }

        Ok(Self {
            value: if s.is_empty() { None } else { Some(s.parse()?) },
            space,
            hidden,
        })
    }
}

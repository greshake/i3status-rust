use std::fmt;
use std::str::FromStr;

use super::prefix::Prefix;
use crate::errors::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    /// `B`
    Bytes,
    /// `b`
    Bits,
    /// `%`
    Percents,
    /// `deg`
    Degrees,
    /// `s`
    Seconds,
    /// `W`
    Watts,
    /// `Hz`
    Hertz,
    /// ``
    None,
}

impl fmt::Display for Unit {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            Self::Bytes => "B",
            Self::Bits => "b",
            Self::Percents => "%",
            Self::Degrees => "°",
            Self::Seconds => "s",
            Self::Watts => "W",
            Self::Hertz => "Hz",
            Self::None => "",
        })
    }
}

impl FromStr for Unit {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "B" => Ok(Unit::Bytes),
            "b" => Ok(Unit::Bits),
            "%" => Ok(Unit::Percents),
            "deg" => Ok(Unit::Degrees),
            "s" => Ok(Unit::Seconds),
            "W" => Ok(Unit::Watts),
            "Hz" => Ok(Unit::Hertz),
            "" => Ok(Unit::None),
            x => Err(Error::new(format!("Unknown unit: '{x}'"))),
        }
    }
}

impl Unit {
    pub fn convert(self, value: f64, unit: Self) -> Result<f64> {
        match (self, unit) {
            (x, y) if x == y => Ok(value),
            (Self::Bytes, Self::Bits) => Ok(value * 8.),
            (Self::Bits, Self::Bytes) => Ok(value / 8.),
            _ => Err(Error::new(format!("Failed to convert '{self}' to '{unit}"))),
        }
    }

    pub fn clamp_prefix(self, prefix: Prefix) -> Prefix {
        match self {
            Self::Bytes | Self::Bits => prefix.max(Prefix::One),
            Self::Percents | Self::Degrees | Self::None => Prefix::One,
            _ => prefix,
        }
    }
}

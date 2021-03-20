use std::convert::TryInto;
use std::fmt;

use crate::errors::*;

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Unit {
    Bytes,
    Bits,
    Percents,
    Degrees,
    Seconds,
    Watts,
    Hertz,
    None,
}

impl fmt::Display for Unit {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Bytes => "B",
                Self::Bits => "b",
                Self::Percents => "%",
                Self::Degrees => "°",
                Self::Seconds => "s",
                Self::Watts => "W",
                Self::Hertz => "Hz",
                Self::None => "",
            }
        )
    }
}

impl TryInto<Unit> for &str {
    type Error = crate::errors::Error;

    fn try_into(self) -> Result<Unit> {
        match self {
            "B" => Ok(Unit::Bytes),
            "b" => Ok(Unit::Bits),
            "%" => Ok(Unit::Percents),
            "deg" => Ok(Unit::Degrees),
            "s" => Ok(Unit::Seconds),
            "W" => Ok(Unit::Watts),
            "Hz" => Ok(Unit::Hertz),
            "" => Ok(Unit::None),
            x => Err(ConfigurationError(
                "Can not parse unit".to_string(),
                format!("unknown unit: '{}'", x.to_string()),
            )),
        }
    }
}

impl Unit {
    //TODO support more complex conversions like Celsius -> Fahrenheit
    pub fn convert(&self, into: Self) -> Result<f64> {
        match self {
            Self::Bits if into == Self::Bytes => Ok(1. / 8.),
            Self::Bytes if into == Self::Bits => Ok(8.),
            x if into == *x || into == Self::None => Ok(1.),
            x => Err(ConfigurationError(
                "Can not convert unit".to_string(),
                format!("it is not possible to convert '{:?}' to '{:?}'", x, into),
            )),
        }
    }

    pub fn is_byte(&self) -> bool {
        matches!(self, Self::Bytes | Self::Bits)
    }
}

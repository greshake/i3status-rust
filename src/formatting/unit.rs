use std::fmt;

use crate::errors::*;

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Unit {
    BitsPerSecond,
    BytesPerSecond,
    Percents,
    Degrees,
    Seconds,
    Watts,
    Hertz,
    Bytes,
    None,
}

impl fmt::Display for Unit {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::BitsPerSecond => "Bi/s",
                Self::BytesPerSecond => "B/s",
                Self::Percents => "%",
                Self::Degrees => "°",
                Self::Seconds => "s",
                Self::Watts => "W",
                Self::Hertz => "Hz",
                Self::Bytes => "B",
                Self::None => "",
            }
        )
    }
}

impl Unit {
    pub fn from_string(s: &str) -> Result<Self> {
        match s {
            "Bi/s" => Ok(Self::BitsPerSecond),
            "B/s" => Ok(Self::BytesPerSecond),
            "%" => Ok(Self::Percents),
            "°" => Ok(Self::Degrees),
            "s" => Ok(Self::Seconds),
            "W" => Ok(Self::Watts),
            "Hz" => Ok(Self::Hertz),
            "B" => Ok(Self::Bytes),
            "" => Ok(Self::None),
            x => Err(ConfigurationError(
                "Can not parse unit".to_string(),
                format!("unknown unit: '{}'", x.to_string()),
            )),
        }
    }

    pub fn convert(&self, into: Self) -> Result<f64> {
        match self {
            Unit::BitsPerSecond if into == Unit::BytesPerSecond => Ok(1. / 8.),
            Unit::BytesPerSecond if into == Unit::BytesPerSecond => Ok(8.),
            x if *x == into => Ok(1.),
            x => Err(ConfigurationError(
                "Can not convert unit".to_string(),
                format!("it is not possible to convert '{:?}' to '{:?}'", x, into),
            )),
        }
    }
}

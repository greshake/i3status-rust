use std::fmt;
use std::str::FromStr;

use crate::errors::*;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Prefix {
    One,
    // SI prefixes
    Nano,
    Micro,
    Milli,
    Kilo,
    Mega,
    Giga,
    Tera,
}

impl fmt::Display for Prefix {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::One => "",
                // SI prefixes
                Self::Nano => "n",
                Self::Micro => "u",
                Self::Milli => "m",
                Self::Kilo => "K",
                Self::Mega => "M",
                Self::Giga => "G",
                Self::Tera => "T",
            }
        )
    }
}

impl FromStr for Prefix {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "1" => Ok(Prefix::One),
            // SI prefixes
            "n" => Ok(Prefix::Nano),
            "u" => Ok(Prefix::Micro),
            "m" => Ok(Prefix::Milli),
            "K" => Ok(Prefix::Kilo),
            "M" => Ok(Prefix::Mega),
            "G" => Ok(Prefix::Giga),
            "T" => Ok(Prefix::Tera),
            x => Err(Error::new(format!("format parser: unknown prefix: '{x}'"))),
        }
    }
}

impl Prefix {
    pub fn convert(&self, into: Self) -> f64 {
        self.to_f64() / into.to_f64()
    }

    pub fn to_f64(self) -> f64 {
        match self {
            Self::One => 1e0,
            // SI prefixes
            Self::Nano => 1e-9,
            Self::Micro => 1e-6,
            Self::Milli => 1e-3,
            Self::Kilo => 1e3,
            Self::Mega => 1e6,
            Self::Giga => 1e9,
            Self::Tera => 1e12,
        }
    }
}

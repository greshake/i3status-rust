use std::convert::TryInto;
use std::fmt;

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

impl TryInto<Prefix> for &str {
    type Error = crate::errors::Error;

    fn try_into(self) -> Result<Prefix> {
        match self {
            "1" => Ok(Prefix::One),
            // SI prefixes
            "n" => Ok(Prefix::Nano),
            "u" => Ok(Prefix::Micro),
            "m" => Ok(Prefix::Milli),
            "K" => Ok(Prefix::Kilo),
            "M" => Ok(Prefix::Mega),
            "G" => Ok(Prefix::Giga),
            "T" => Ok(Prefix::Tera),
            x => Err(ConfigurationError(
                "Can not parse prefix".to_string(),
                format!("unknown prefix: '{}'", x.to_string()),
            )),
        }
    }
}

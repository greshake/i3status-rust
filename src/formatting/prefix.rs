use std::fmt;

use crate::errors::*;

#[derive(Debug, Clone, Copy)]
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

impl Prefix {
    pub fn from_string(s: &str) -> Result<Self> {
        match s {
            "1" => Ok(Self::One),
            // SI prefixes
            "n" => Ok(Self::Nano),
            "u" => Ok(Self::Micro),
            "m" => Ok(Self::Milli),
            "K" => Ok(Self::Kilo),
            "M" => Ok(Self::Mega),
            "G" => Ok(Self::Giga),
            "T" => Ok(Self::Tera),
            x => Err(ConfigurationError(
                "Can not parse prefix".to_string(),
                format!("unknown prefix: '{}'", x.to_string()),
            )),
        }
    }
}

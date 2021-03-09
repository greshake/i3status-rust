use std::fmt;

use crate::errors::*;

#[derive(Debug, Clone)]
pub enum Suffix {
    Nano,
    Micro,
    Milli,
    One,
    Kilo,
    Mega,
    Giga,
    Tera,
}

impl fmt::Display for Suffix {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Nano => "n",
                Self::Micro => "u",
                Self::Milli => "m",
                Self::One => "",
                Self::Kilo => "K",
                Self::Mega => "M",
                Self::Giga => "G",
                Self::Tera => "T",
            }
        )
    }
}

impl Suffix {
    pub fn from_string(s: &str) -> Result<Self> {
        match s {
            "n" => Ok(Self::Nano),
            "u" => Ok(Self::Micro),
            "m" => Ok(Self::Milli),
            "1" => Ok(Self::One),
            "K" => Ok(Self::Kilo),
            "M" => Ok(Self::Mega),
            "G" => Ok(Self::Giga),
            "T" => Ok(Self::Tera),
            x => Err(ConfigurationError(
                "Can not parse suffix".to_string(),
                format!("unknown suffix: '{}'", x.to_string()),
            )),
        }
    }
}

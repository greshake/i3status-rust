use std::fmt;

use crate::errors::*;

#[derive(Debug, Clone, Copy)]
pub enum Suffix {
    One,
    // SI
    Nano,
    Micro,
    Milli,
    Kilo,
    Mega,
    Giga,
    Tera,
    // Bytes
    Ki,
    Mi,
    Gi,
    Ti,
}

impl fmt::Display for Suffix {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::One => "",
                // SI
                Self::Nano => "n",
                Self::Micro => "u",
                Self::Milli => "m",
                Self::Kilo => "K",
                Self::Mega => "M",
                Self::Giga => "G",
                Self::Tera => "T",
                // Bytes
                Self::Ki => "Ki",
                Self::Mi => "Mi",
                Self::Gi => "Gi",
                Self::Ti => "Ti",
            }
        )
    }
}

impl Suffix {
    pub fn from_string(s: &str) -> Result<Self> {
        match s {
            "1" => Ok(Self::One),
            // SI
            "n" => Ok(Self::Nano),
            "u" => Ok(Self::Micro),
            "m" => Ok(Self::Milli),
            "K" => Ok(Self::Kilo),
            "M" => Ok(Self::Mega),
            "G" => Ok(Self::Giga),
            "T" => Ok(Self::Tera),
            // Bytes
            "Ki" => Ok(Self::Ki),
            "Mi" => Ok(Self::Mi),
            "Gi" => Ok(Self::Gi),
            "Ti" => Ok(Self::Ti),
            x => Err(ConfigurationError(
                "Can not parse suffix".to_string(),
                format!("unknown suffix: '{}'", x.to_string()),
            )),
        }
    }
}

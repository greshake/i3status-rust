use crate::errors::*;
use std::fmt;
use std::str::FromStr;

/// SI prefix
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Prefix {
    /// `n`
    Nano,
    /// `u`
    Micro,
    /// `m`
    Milli,
    /// `1`
    One,
    /// `1i`
    OneButBinary,
    /// `K`
    Kilo,
    /// `Ki`
    Kibi,
    /// `M`
    Mega,
    /// `Mi`
    Mebi,
    /// `G`
    Giga,
    /// `Gi`
    Gibi,
    /// `T`
    Tera,
    /// `Ti`
    Tebi,
}

const MUL: [f64; 13] = [
    1e-9,
    1e-6,
    1e-3,
    1.0,
    1.0,
    1e3,
    1024.0,
    1e6,
    1024.0 * 1024.0,
    1e9,
    1024.0 * 1024.0 * 1024.0,
    1e12,
    1024.0 * 1024.0 * 1024.0 * 1024.0,
];

impl Prefix {
    pub fn min_available() -> Self {
        Self::Nano
    }

    pub fn max_available() -> Self {
        Self::Tera
    }

    pub fn max(self, other: Self) -> Self {
        if other > self {
            other
        } else {
            self
        }
    }

    pub fn apply(self, value: f64) -> f64 {
        value / MUL[self as usize]
    }

    pub fn eng(number: f64) -> Self {
        if number == 0.0 {
            Self::One
        } else {
            match number.abs().log10().div_euclid(3.) as i32 {
                i32::MIN..=-3 => Prefix::Nano,
                -2 => Prefix::Micro,
                -1 => Prefix::Milli,
                0 => Prefix::One,
                1 => Prefix::Kilo,
                2 => Prefix::Mega,
                3 => Prefix::Giga,
                4..=i32::MAX => Prefix::Tera,
            }
        }
    }

    pub fn eng_binary(number: f64) -> Self {
        if number == 0.0 {
            Self::One
        } else {
            match number.abs().log2().div_euclid(10.) as i32 {
                i32::MIN..=0 => Prefix::OneButBinary,
                1 => Prefix::Kibi,
                2 => Prefix::Mebi,
                3 => Prefix::Gibi,
                4..=i32::MAX => Prefix::Tebi,
            }
        }
    }

    pub fn is_binary(&self) -> bool {
        matches!(
            self,
            Self::OneButBinary | Self::Kibi | Self::Mebi | Self::Gibi | Self::Tebi
        )
    }
}

impl fmt::Display for Prefix {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            Self::Nano => "n",
            Self::Micro => "u",
            Self::Milli => "m",
            Self::One | Self::OneButBinary => "",
            Self::Kilo => "K",
            Self::Kibi => "Ki",
            Self::Mega => "M",
            Self::Mebi => "Mi",
            Self::Giga => "G",
            Self::Gibi => "Gi",
            Self::Tera => "T",
            Self::Tebi => "Ti",
        })
    }
}

impl FromStr for Prefix {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "n" => Ok(Prefix::Nano),
            "u" => Ok(Prefix::Micro),
            "m" => Ok(Prefix::Milli),
            "1" => Ok(Prefix::One),
            "1i" => Ok(Prefix::OneButBinary),
            "K" => Ok(Prefix::Kilo),
            "Ki" => Ok(Prefix::Kibi),
            "M" => Ok(Prefix::Mega),
            "Mi" => Ok(Prefix::Mebi),
            "G" => Ok(Prefix::Giga),
            "Gi" => Ok(Prefix::Gibi),
            "T" => Ok(Prefix::Tera),
            "Ti" => Ok(Prefix::Tebi),
            x => Err(Error::new(format!("Unknown prefix: '{}'", x))),
        }
    }
}

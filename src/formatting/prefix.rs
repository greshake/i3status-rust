use std::fmt;
use std::ops::AddAssign;
use std::str::FromStr;

use crate::errors::*;

/// SI prefix
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Prefix {
    /// `n`
    Nano = -3,
    /// `u`
    Micro = -2,
    /// `m`
    Milli = -1,
    /// `1`
    One = 0,
    /// `K`
    Kilo = 1,
    /// `M`
    Mega = 2,
    /// `G`
    Giga = 3,
    /// `T`
    Tera = 4,
}

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

    pub fn clamp(self, min: Self, max: Self) -> Self {
        if self < min {
            min
        } else if self > max {
            max
        } else {
            self
        }
    }

    pub fn apply(self, value: f64) -> f64 {
        value / 1_000f64.powi(self as i32)
    }

    pub fn from_exp_level(exp_level: i32) -> Self {
        match exp_level {
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

impl AddAssign<i32> for Prefix {
    fn add_assign(&mut self, rhs: i32) {
        *self = Self::from_exp_level(*self as i32 + rhs);
    }
}

impl fmt::Display for Prefix {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            Self::Nano => "n",
            Self::Micro => "u",
            Self::Milli => "m",
            Self::One => "",
            Self::Kilo => "K",
            Self::Mega => "M",
            Self::Giga => "G",
            Self::Tera => "T",
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
            "K" => Ok(Prefix::Kilo),
            "M" => Ok(Prefix::Mega),
            "G" => Ok(Prefix::Giga),
            "T" => Ok(Prefix::Tera),
            x => Err(Error::new(format!("Unknown prefix: '{}'", x))),
        }
    }
}

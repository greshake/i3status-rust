use crate::errors::*;
use nom::IResult;
use nom::Parser as _;
use nom::branch::alt;
use nom::bytes::complete::tag;
use nom::combinator::{all_consuming, map, opt, value};
use nom::number::complete::double;
use std::fmt;
use std::str::FromStr;

/// SI prefix
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Prefix {
    /// `n`
    Nano,
    /// `u`
    Micro,
    /// `m`
    Milli,
    /// `1`
    #[default]
    One,
    /// `1i`
    /// `1i` is a special prefix which means "one but binary". `1i` is to `1` as `Ki` is to `K`.
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
    #[inline]
    pub const fn min_available() -> Self {
        Self::Nano
    }

    #[inline]
    pub const fn max_available() -> Self {
        Self::Tebi
    }

    #[inline]
    pub fn max(self, other: Self) -> Self {
        if other > self { other } else { self }
    }

    #[inline]
    pub const fn apply(self, value: f64) -> f64 {
        value / MUL[self as usize]
    }

    #[inline]
    pub const fn unapply(self, value: f64) -> f64 {
        value * MUL[self as usize]
    }

    pub fn eng(mut number: f64) -> Self {
        if number == 0.0 {
            Self::One
        } else {
            number = number.abs();
            if number > 1.0 {
                number = number.round();
            } else {
                let round_up_to = -(number.log10().ceil() as i32);
                let m = 10f64.powi(round_up_to);
                number = (number * m).round() / m;
            }
            match number.log10().div_euclid(3.) as i32 {
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
            match number.abs().round().log2().div_euclid(10.) as i32 {
                i32::MIN..=0 => Prefix::OneButBinary,
                1 => Prefix::Kibi,
                2 => Prefix::Mebi,
                3 => Prefix::Gibi,
                4..=i32::MAX => Prefix::Tebi,
            }
        }
    }

    #[inline]
    pub const fn is_binary(&self) -> bool {
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
            x => Err(Error::new(format!("Unknown prefix: '{x}'"))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct ValuePrefix(pub f64, pub Prefix);

impl ValuePrefix {
    #[inline]
    pub const fn value(&self) -> f64 {
        self.0
    }

    #[inline]
    pub const fn prefix(&self) -> Prefix {
        self.1
    }

    #[inline]
    pub const fn result(&self) -> f64 {
        self.prefix().unapply(self.value())
    }
}

impl fmt::Display for ValuePrefix {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}{}", self.value(), self.prefix())
    }
}

impl FromStr for ValuePrefix {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_value_prefix(s)
            .map(|(_, v)| v)
            .map_err(|e| Error::new(format!("Failed to parse value prefix: {e}")))
    }
}

fn parse_prefix(input: &str) -> IResult<&str, Prefix> {
    map(
        opt(alt((
            value(Prefix::Kibi, tag("Ki")),
            value(Prefix::Mebi, tag("Mi")),
            value(Prefix::Gibi, tag("Gi")),
            value(Prefix::Tebi, tag("Ti")),
            value(Prefix::Nano, tag("n")),
            value(Prefix::Micro, tag("u")),
            value(Prefix::Milli, tag("m")),
            value(Prefix::OneButBinary, tag("i")),
            value(Prefix::Kilo, tag("K")),
            value(Prefix::Mega, tag("M")),
            value(Prefix::Giga, tag("G")),
            value(Prefix::Tera, tag("T")),
        ))),
        |p| p.unwrap_or_default(),
    )
    .parse(input)
}

fn parse_value_prefix(input: &str) -> IResult<&str, ValuePrefix> {
    all_consuming(map((double, parse_prefix), |(v, p)| ValuePrefix(v, p))).parse(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eng() {
        assert_eq!(Prefix::eng(0.000_000_000_1), Prefix::Nano);
        assert_eq!(Prefix::eng(0.000_000_001), Prefix::Nano);
        assert_eq!(Prefix::eng(0.000_000_01), Prefix::Nano);
        assert_eq!(Prefix::eng(0.000_000_1), Prefix::Nano);
        assert_eq!(Prefix::eng(0.000_001), Prefix::Micro);
        assert_eq!(Prefix::eng(0.000_01), Prefix::Micro);
        assert_eq!(Prefix::eng(0.000_1), Prefix::Micro);
        assert_eq!(Prefix::eng(0.001), Prefix::Milli);
        assert_eq!(Prefix::eng(0.01), Prefix::Milli);
        assert_eq!(Prefix::eng(0.1), Prefix::Milli);
        assert_eq!(Prefix::eng(1.0), Prefix::One);
        assert_eq!(Prefix::eng(10.0), Prefix::One);
        assert_eq!(Prefix::eng(100.0), Prefix::One);
        assert_eq!(Prefix::eng(1_000.0), Prefix::Kilo);
        assert_eq!(Prefix::eng(10_000.0), Prefix::Kilo);
        assert_eq!(Prefix::eng(100_000.0), Prefix::Kilo);
        assert_eq!(Prefix::eng(1_000_000.0), Prefix::Mega);
        assert_eq!(Prefix::eng(10_000_000.0), Prefix::Mega);
        assert_eq!(Prefix::eng(100_000_000.0), Prefix::Mega);
        assert_eq!(Prefix::eng(1_000_000_000.0), Prefix::Giga);
        assert_eq!(Prefix::eng(10_000_000_000.0), Prefix::Giga);
        assert_eq!(Prefix::eng(100_000_000_000.0), Prefix::Giga);
        assert_eq!(Prefix::eng(1_000_000_000_000.0), Prefix::Tera);
        assert_eq!(Prefix::eng(10_000_000_000_000.0), Prefix::Tera);
        assert_eq!(Prefix::eng(100_000_000_000_000.0), Prefix::Tera);
        assert_eq!(Prefix::eng(1_000_000_000_000_000.0), Prefix::Tera);
    }

    #[test]
    fn eng_round() {
        assert_eq!(Prefix::eng(0.000_000_000_09), Prefix::Nano);
        assert_eq!(Prefix::eng(0.000_000_000_9), Prefix::Nano);
        assert_eq!(Prefix::eng(0.000_000_009), Prefix::Nano);
        assert_eq!(Prefix::eng(0.000_000_09), Prefix::Nano);
        assert_eq!(Prefix::eng(0.000_000_9), Prefix::Micro);
        assert_eq!(Prefix::eng(0.000_009), Prefix::Micro);
        assert_eq!(Prefix::eng(0.000_09), Prefix::Micro);
        assert_eq!(Prefix::eng(0.000_9), Prefix::Milli);
        assert_eq!(Prefix::eng(0.009), Prefix::Milli);
        assert_eq!(Prefix::eng(0.09), Prefix::Milli);
        assert_eq!(Prefix::eng(0.9), Prefix::One);
        assert_eq!(Prefix::eng(9.9), Prefix::One);
        assert_eq!(Prefix::eng(99.9), Prefix::One);
        assert_eq!(Prefix::eng(999.9), Prefix::Kilo);
        assert_eq!(Prefix::eng(9_999.9), Prefix::Kilo);
        assert_eq!(Prefix::eng(99_999.9), Prefix::Kilo);
        assert_eq!(Prefix::eng(999_999.9), Prefix::Mega);
        assert_eq!(Prefix::eng(9_999_999.9), Prefix::Mega);
        assert_eq!(Prefix::eng(99_999_999.9), Prefix::Mega);
        assert_eq!(Prefix::eng(999_999_999.9), Prefix::Giga);
        assert_eq!(Prefix::eng(9_999_999_999.9), Prefix::Giga);
        assert_eq!(Prefix::eng(99_999_999_999.9), Prefix::Giga);
        assert_eq!(Prefix::eng(999_999_999_999.9), Prefix::Tera);
        assert_eq!(Prefix::eng(9_999_999_999_999.9), Prefix::Tera);
        assert_eq!(Prefix::eng(99_999_999_999_999.9), Prefix::Tera);
        assert_eq!(Prefix::eng(999_999_999_999_999.9), Prefix::Tera);
    }

    #[test]
    fn eng_binary() {
        assert_eq!(Prefix::eng_binary(0.1), Prefix::OneButBinary);
        assert_eq!(Prefix::eng_binary(1.0), Prefix::OneButBinary);
        assert_eq!(Prefix::eng_binary((1 << 9) as f64), Prefix::OneButBinary);
        assert_eq!(Prefix::eng_binary((1 << 10) as f64), Prefix::Kibi);
        assert_eq!(Prefix::eng_binary((1 << 19) as f64), Prefix::Kibi);
        assert_eq!(Prefix::eng_binary((1 << 29) as f64), Prefix::Mebi);
        assert_eq!(Prefix::eng_binary((1 << 20) as f64), Prefix::Mebi);
        assert_eq!(Prefix::eng_binary((1 << 30) as f64), Prefix::Gibi);
        assert_eq!(Prefix::eng_binary((1_u64 << 39) as f64), Prefix::Gibi);
        assert_eq!(Prefix::eng_binary((1_u64 << 40) as f64), Prefix::Tebi);
        assert_eq!(Prefix::eng_binary((1_u64 << 49) as f64), Prefix::Tebi);
        assert_eq!(Prefix::eng_binary((1_u64 << 50) as f64), Prefix::Tebi);
    }

    #[test]
    fn eng_binary_round() {
        assert_eq!(Prefix::eng_binary(0.9), Prefix::OneButBinary);
        assert_eq!(
            Prefix::eng_binary((1 << 9) as f64 - 0.1),
            Prefix::OneButBinary
        );
        assert_eq!(Prefix::eng_binary((1 << 10) as f64 - 0.1), Prefix::Kibi);
        assert_eq!(Prefix::eng_binary((1 << 19) as f64 - 0.1), Prefix::Kibi);
        assert_eq!(Prefix::eng_binary((1 << 29) as f64 - 0.1), Prefix::Mebi);
        assert_eq!(Prefix::eng_binary((1 << 20) as f64 - 0.1), Prefix::Mebi);
        assert_eq!(Prefix::eng_binary((1 << 30) as f64 - 0.1), Prefix::Gibi);
        assert_eq!(Prefix::eng_binary((1_u64 << 39) as f64 - 0.1), Prefix::Gibi);
        assert_eq!(Prefix::eng_binary((1_u64 << 40) as f64 - 0.1), Prefix::Tebi);
        assert_eq!(Prefix::eng_binary((1_u64 << 49) as f64 - 0.1), Prefix::Tebi);
        assert_eq!(Prefix::eng_binary((1_u64 << 50) as f64 - 0.1), Prefix::Tebi);
    }

    #[test]
    fn value_prefix() -> Result<()> {
        assert_eq!(ValuePrefix::from_str("1")?.result(), 1.0);
        assert_eq!(ValuePrefix::from_str("1G")?.result(), 1e9);
        assert_eq!(ValuePrefix::from_str("1e9")?.result(), 1e9);
        assert_eq!(ValuePrefix::from_str("10e9")?.result(), 10e9);
        assert_eq!(ValuePrefix::from_str("10Gi")?.result(), 10737418240.0);
        assert_eq!(ValuePrefix::from_str("10M")?.result(), 1e7);

        Ok(())
    }
}

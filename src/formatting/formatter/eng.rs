use crate::formatting::prefix::Prefix;
use crate::formatting::unit::Unit;

use std::borrow::Cow;
use std::ops::RangeInclusive;

use super::*;

const DEFAULT_NUMBER_WIDTH: usize = 2;

pub const DEFAULT_NUMBER_FORMATTER: EngFormatter = EngFormatter {
    show: true,
    width: DEFAULT_NUMBER_WIDTH,
    unit: None,
    unit_has_space: false,
    unit_hidden: false,
    prefix: None,
    prefix_has_space: false,
    prefix_hidden: false,
    prefix_forced: false,
    pad_with: DEFAULT_NUMBER_PAD_WITH,
    range: f64::NEG_INFINITY..=f64::INFINITY,
};

#[derive(Debug)]
pub struct EngFormatter {
    show: bool,
    width: usize,
    unit: Option<Unit>,
    unit_has_space: bool,
    unit_hidden: bool,
    prefix: Option<Prefix>,
    prefix_has_space: bool,
    prefix_hidden: bool,
    prefix_forced: bool,
    pad_with: PadWith,
    range: RangeInclusive<f64>,
}

impl EngFormatter {
    pub(super) fn from_args(args: &[Arg]) -> Result<Self> {
        let mut result = DEFAULT_NUMBER_FORMATTER;

        for arg in args {
            match arg.key {
                "width" | "w" => {
                    result.width = arg.parse_value()?;
                }
                "unit" | "u" => {
                    result.unit = Some(arg.parse_value()?);
                }
                "hide_unit" => {
                    result.unit_hidden = arg.parse_value()?;
                }
                "unit_space" => {
                    result.unit_has_space = arg.parse_value()?;
                }
                "prefix" | "p" => {
                    result.prefix = Some(arg.parse_value()?);
                }
                "hide_prefix" => {
                    result.prefix_hidden = arg.parse_value()?;
                }
                "prefix_space" => {
                    result.prefix_has_space = arg.parse_value()?;
                }
                "force_prefix" => {
                    result.prefix_forced = arg.parse_value()?;
                }
                "pad_with" => {
                    let pad_with_str = arg.val.error("pad_with must be specified")?;
                    if pad_with_str.graphemes(true).count() < 2 {
                        result.pad_with = Cow::Owned(pad_with_str.into());
                    } else {
                        return Err(Error::new(
                            "pad_with must be an empty string or a single character",
                        ));
                    }
                }
                "range" => {
                    let (start, end) = arg
                        .val
                        .error("range must be specified")?
                        .split_once("..")
                        .error("invalid range")?;
                    if !start.is_empty() {
                        result.range = start.parse::<f64>().error("invalid range start")?
                            ..=*result.range.end();
                    }
                    if !end.is_empty() {
                        result.range = *result.range.start()
                            ..=end.parse::<f64>().error("invalid range end")?;
                    }
                }
                "show" => {
                    result.show = arg.parse_value()?;
                }
                other => {
                    return Err(Error::new(format!("Unknown argument for 'eng': '{other}'")));
                }
            }
        }

        Ok(result)
    }
}

impl Formatter for EngFormatter {
    fn format(&self, val: &Value, _config: &SharedConfig) -> Result<String, FormatError> {
        match val {
            &Value::Number { mut val, mut unit } => {
                if !self.range.contains(&val) {
                    return Err(FormatError::NumberOutOfRange(val));
                }

                if !self.show {
                    return Ok(String::new());
                }

                let is_negative = val.is_sign_negative();
                if is_negative {
                    val = -val;
                }

                if let Some(new_unit) = self.unit {
                    val = unit.convert(val, new_unit)?;
                    unit = new_unit;
                }

                let (min_prefix, max_prefix) = match (self.prefix, self.prefix_forced) {
                    (Some(prefix), true) => (prefix, prefix),
                    (Some(prefix), false) => (prefix, Prefix::max_available()),
                    (None, _) => (Prefix::min_available(), Prefix::max_available()),
                };

                let mut prefix = unit
                    .clamp_prefix(if min_prefix.is_binary() {
                        Prefix::eng_binary(val)
                    } else {
                        Prefix::eng(val)
                    })
                    .clamp(min_prefix, max_prefix);
                val = prefix.apply(val);

                let mut digits = (val.max(1.).log10().floor() + 1.0) as i32 + is_negative as i32;

                // handle rounding
                if self.width as i32 - digits >= 1 {
                    let round_up_to = self.width as i32 - digits - 1;
                    let m = 10f64.powi(round_up_to);
                    val = (val * m).round() / m;
                    digits = (val.max(1.).log10().floor() + 1.0) as i32 + is_negative as i32;
                }

                let sign = if is_negative { "-" } else { "" };
                let mut retval = match self.width as i32 - digits {
                    i32::MIN..=1 => {
                        // Remove prefix after rounding value to be displayed
                        val = prefix.unapply(val.round());

                        prefix = unit
                            .clamp_prefix(if min_prefix.is_binary() {
                                Prefix::eng_binary(val)
                            } else {
                                Prefix::eng(val)
                            })
                            .clamp(min_prefix, max_prefix);
                        val = prefix.apply(val);

                        digits = (val.max(1.).log10().floor() + 1.0) as i32 + is_negative as i32;

                        match self.width as i32 - digits {
                            i32::MIN..=0 => format!("{sign}{}", val),
                            1 => format!("{}{sign}{}", self.pad_with, val as i64),
                            rest => format!("{sign}{val:.*}", rest as usize - 1),
                        }
                    }
                    rest => format!("{sign}{val:.*}", rest as usize - 1),
                };

                let display_prefix =
                    !self.prefix_hidden && prefix != Prefix::One && prefix != Prefix::OneButBinary;
                let display_unit = !self.unit_hidden && unit != Unit::None;

                if display_prefix {
                    if self.prefix_has_space {
                        retval.push(' ');
                    }
                    retval.push_str(&prefix.to_string());
                }
                if display_unit {
                    if self.unit_has_space || (self.prefix_has_space && !display_prefix) {
                        retval.push(' ');
                    }
                    retval.push_str(&unit.to_string());
                }

                Ok(retval)
            }
            &Value::Numbers { ref vals, unit } => {
                // NOTE: by default, Value::Numbers will be formatted as the last value in the series
                Ok(self.format(
                    &Value::Number {
                        val: vals.last().copied().unwrap_or(0.0),
                        unit,
                    },
                    _config,
                )?)
            }
            other => Err(FormatError::IncompatibleFormatter {
                ty: other.type_name(),
                fmt: "eng",
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eng_rounding_and_negatives() {
        let fmt = new_fmt!(eng, w: 3).unwrap();
        let config = SharedConfig::default();

        let result = fmt
            .format(
                &Value::Number {
                    val: -1.0,
                    unit: Unit::None,
                },
                &config,
            )
            .unwrap();
        assert_eq!(result, " -1");

        let result = fmt
            .format(
                &Value::Number {
                    val: 9.9999,
                    unit: Unit::None,
                },
                &config,
            )
            .unwrap();
        assert_eq!(result, " 10");

        let result = fmt
            .format(
                &Value::Number {
                    val: 999.9,
                    unit: Unit::Bytes,
                },
                &config,
            )
            .unwrap();
        assert_eq!(result, "1.0KB");

        let result = fmt
            .format(
                &Value::Number {
                    val: -9.99,
                    unit: Unit::None,
                },
                &config,
            )
            .unwrap();
        assert_eq!(result, "-10");

        let result = fmt
            .format(
                &Value::Number {
                    val: 9.94,
                    unit: Unit::None,
                },
                &config,
            )
            .unwrap();
        assert_eq!(result, "9.9");

        let result = fmt
            .format(
                &Value::Number {
                    val: 9.95,
                    unit: Unit::None,
                },
                &config,
            )
            .unwrap();
        assert_eq!(result, " 10");

        let fmt = new_fmt!(eng, w: 5, p: 1).unwrap();
        let result = fmt
            .format(
                &Value::Number {
                    val: 321_600_000_000.,
                    unit: Unit::Bytes,
                },
                &config,
            )
            .unwrap();
        assert_eq!(result, "321.6GB");

        let fmt = new_fmt!(eng, w: 3, p: K).unwrap();
        let result = fmt
            .format(
                &Value::Number {
                    val: 998_888.,
                    unit: Unit::Bytes,
                },
                &config,
            )
            .unwrap();
        assert_eq!(result, "999KB");

        let result = fmt
            .format(
                &Value::Number {
                    val: 999_888.,
                    unit: Unit::Bytes,
                },
                &config,
            )
            .unwrap();
        assert_eq!(result, "1.0MB");

        let result = fmt
            .format(
                &Value::Number {
                    val: 1_000_000.,
                    unit: Unit::Bytes,
                },
                &config,
            )
            .unwrap();
        assert_eq!(result, "1.0MB");
    }

    #[test]
    fn eng_prefixes() {
        let config = SharedConfig::default();
        // 14.96 GiB
        let val = Value::Number {
            val: 14.96 * 1024. * 1024. * 1024.,
            unit: Unit::Bytes,
        };

        let fmt = new_fmt!(eng, w: 5, p: Mi).unwrap();
        let result = fmt.format(&val, &config).unwrap();
        assert_eq!(result, "14.96GiB");

        let fmt = new_fmt!(eng, w: 4, p: Mi).unwrap();
        let result = fmt.format(&val, &config).unwrap();
        assert_eq!(result, "15.0GiB");

        let fmt = new_fmt!(eng, w: 3, p: Mi).unwrap();
        let result = fmt.format(&val, &config).unwrap();
        assert_eq!(result, " 15GiB");

        let fmt = new_fmt!(eng, w: 2, p: Mi).unwrap();
        let result = fmt.format(&val, &config).unwrap();
        assert_eq!(result, "15GiB");
    }
}

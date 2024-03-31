use std::str::FromStr;

use crate::formatting::unit::Unit;

use super::*;

#[derive(Debug)]
enum Style {
    ChineseCountingRods,
    ChineseTally,
    WesternTally,
    WesternTallyUngrouped,
}

impl FromStr for Style {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "chinese_counting_rods" | "ccr" => Ok(Style::ChineseCountingRods),
            "chinese_tally" | "ct" => Ok(Style::ChineseTally),
            "western_tally" | "wt" => Ok(Style::WesternTally),
            "western_tally_ungrouped" | "wtu" => Ok(Style::WesternTallyUngrouped),
            x => Err(Error::new(format!("Unknown Style: '{x}'"))),
        }
    }
}

#[derive(Debug)]
pub struct TallyFormatter {
    style: Style,
}

impl TallyFormatter {
    pub(super) fn from_args(args: &[Arg]) -> Result<Self> {
        let mut style = Style::WesternTally;
        for arg in args {
            match arg.key {
                "style" | "s" => {
                    style = arg.val.parse()?;
                }
                other => {
                    return Err(Error::new(format!(
                        "Unknown argument for 'tally': '{other}'"
                    )));
                }
            }
        }
        Ok(Self { style })
    }
}

const HORIZONTAL_CHINESE_COUNTING_RODS_CHARS: [char; 10] =
    ['〇', '𝍠', '𝍡', '𝍢', '𝍣', '𝍤', '𝍥', '𝍦', '𝍧', '𝍨'];

const VERTICAL_CHINESE_COUNTING_RODS_CHARS: [char; 10] =
    ['〇', '𝍩', '𝍪', '𝍫', '𝍬', '𝍭', '𝍮', '𝍯', '𝍰', '𝍱'];

const CHINESE_TALLY_CHARS: [char; 5] = ['𝍲', '𝍳', '𝍴', '𝍵', '𝍶'];

impl Formatter for TallyFormatter {
    fn format(&self, val: &Value, _config: &SharedConfig) -> Result<String, FormatError> {
        match val {
            Value::Number {
                val,
                unit: Unit::None,
            } => {
                let is_negative = val.is_sign_negative();
                let mut val = val.abs().round() as u64;
                let mut result = String::new();
                match self.style {
                    Style::ChineseCountingRods => {
                        if is_negative {
                            result.push('\u{20E5}');
                        }
                        if val == 0 {
                            result.insert(0, '〇');
                        } else {
                            let mut horizontal = true;
                            while val != 0 {
                                let digit = val % 10;
                                val /= 10;
                                let charset = if horizontal {
                                    horizontal = false;
                                    HORIZONTAL_CHINESE_COUNTING_RODS_CHARS
                                } else {
                                    horizontal = true;
                                    VERTICAL_CHINESE_COUNTING_RODS_CHARS
                                };
                                result.insert(0, charset[digit as usize]);
                            }
                        }
                    }
                    Style::ChineseTally => {
                        if is_negative {
                            return Err(FormatError::Other(Error::new(
                                "Chinese Tally marks do not support negative numbers",
                            )));
                        }
                        let (fives, rem) = (val / 5, val % 5);
                        for _ in 0..fives {
                            result.push(CHINESE_TALLY_CHARS[4]);
                        }
                        if rem != 0 {
                            result.push(CHINESE_TALLY_CHARS[rem as usize - 1]);
                        }
                    }
                    Style::WesternTally | Style::WesternTallyUngrouped => {
                        if is_negative {
                            return Err(FormatError::Other(Error::new(
                                "Western Tally marks do not support negative numbers",
                            )));
                        }
                        if matches!(self.style, Style::WesternTally) {
                            let fives = val / 5;
                            val %= 5;
                            for _ in 0..fives {
                                result.push('𝍸');
                            }
                        }
                        for _ in 0..val {
                            result.push('𝍷');
                        }
                    }
                }
                Ok(result)
            }
            Value::Number { .. } => Err(FormatError::Other(Error::new(
                "Tally can only format Numbers with Unit::None",
            ))),
            other => Err(FormatError::IncompatibleFormatter {
                ty: other.type_name(),
                fmt: "tally",
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tally_chinese_counting_rods_negative() {
        let fmt = new_fmt!(tally, style: chinese_counting_rods).unwrap();
        let config = SharedConfig::default();

        let result = fmt
            .format(
                &Value::Number {
                    val: -0.0,
                    unit: Unit::None,
                },
                &config,
            )
            .unwrap();
        assert_eq!(result, "〇\u{20E5}");

        for (hundreds, hundreds_char) in HORIZONTAL_CHINESE_COUNTING_RODS_CHARS
            .into_iter()
            .enumerate()
        {
            for (tens, tens_char) in VERTICAL_CHINESE_COUNTING_RODS_CHARS.into_iter().enumerate() {
                for (ones, ones_char) in HORIZONTAL_CHINESE_COUNTING_RODS_CHARS
                    .into_iter()
                    .enumerate()
                {
                    let val = -((hundreds * 100 + tens * 10 + ones) as f64);
                    if val == 0.0 {
                        continue;
                    }
                    // Contcat characters, excluding leading 〇
                    let expected = String::from_iter(
                        [hundreds_char, tens_char, ones_char, '\u{20E5}']
                            .into_iter()
                            .skip_while(|c| *c == '〇'),
                    );

                    let result = fmt
                        .format(
                            &Value::Number {
                                val,
                                unit: Unit::None,
                            },
                            &config,
                        )
                        .unwrap();
                    assert_eq!(result, expected);
                }
            }
        }
    }

    #[test]
    fn tally_chinese_counting_rods_positive() {
        let fmt = new_fmt!(tally, style: chinese_counting_rods).unwrap();
        let config = SharedConfig::default();

        let result = fmt
            .format(
                &Value::Number {
                    val: 0.0,
                    unit: Unit::None,
                },
                &config,
            )
            .unwrap();
        assert_eq!(result, "〇");

        for (hundreds, hundreds_char) in HORIZONTAL_CHINESE_COUNTING_RODS_CHARS
            .into_iter()
            .enumerate()
        {
            for (tens, tens_char) in VERTICAL_CHINESE_COUNTING_RODS_CHARS.into_iter().enumerate() {
                for (ones, ones_char) in HORIZONTAL_CHINESE_COUNTING_RODS_CHARS
                    .into_iter()
                    .enumerate()
                {
                    let val = (hundreds * 100 + tens * 10 + ones) as f64;
                    if val == 0.0 {
                        continue;
                    }
                    // Contcat characters, excluding leading 〇
                    let expected = String::from_iter(
                        [hundreds_char, tens_char, ones_char]
                            .into_iter()
                            .skip_while(|c| *c == '〇'),
                    );

                    let result = fmt
                        .format(
                            &Value::Number {
                                val,
                                unit: Unit::None,
                            },
                            &config,
                        )
                        .unwrap();
                    assert_eq!(result, expected);
                }
            }
        }
    }

    #[test]
    fn tally_chinese_tally_negative() {
        let fmt = new_fmt!(tally, style: chinese_tally).unwrap();
        let config = SharedConfig::default();

        let result = fmt.format(
            &Value::Number {
                val: -1.0,
                unit: Unit::None,
            },
            &config,
        );
        assert!(result.is_err());
    }

    #[test]
    fn tally_chinese_tally_positive() {
        let fmt = new_fmt!(tally, style: chinese_tally).unwrap();
        let config = SharedConfig::default();

        let result = fmt
            .format(
                &Value::Number {
                    val: 0.0,
                    unit: Unit::None,
                },
                &config,
            )
            .unwrap();
        assert_eq!(result, "");

        let result = fmt
            .format(
                &Value::Number {
                    val: 1.0,
                    unit: Unit::None,
                },
                &config,
            )
            .unwrap();
        assert_eq!(result, "𝍲");

        let result = fmt
            .format(
                &Value::Number {
                    val: 2.0,
                    unit: Unit::None,
                },
                &config,
            )
            .unwrap();
        assert_eq!(result, "𝍳");

        let result = fmt
            .format(
                &Value::Number {
                    val: 3.0,
                    unit: Unit::None,
                },
                &config,
            )
            .unwrap();
        assert_eq!(result, "𝍴");

        let result = fmt
            .format(
                &Value::Number {
                    val: 4.0,
                    unit: Unit::None,
                },
                &config,
            )
            .unwrap();
        assert_eq!(result, "𝍵");

        let result = fmt
            .format(
                &Value::Number {
                    val: 5.0,
                    unit: Unit::None,
                },
                &config,
            )
            .unwrap();
        assert_eq!(result, "𝍶");

        let result = fmt
            .format(
                &Value::Number {
                    val: 6.0,
                    unit: Unit::None,
                },
                &config,
            )
            .unwrap();
        assert_eq!(result, "𝍶𝍲");

        let result = fmt
            .format(
                &Value::Number {
                    val: 7.0,
                    unit: Unit::None,
                },
                &config,
            )
            .unwrap();
        assert_eq!(result, "𝍶𝍳");

        let result = fmt
            .format(
                &Value::Number {
                    val: 8.0,
                    unit: Unit::None,
                },
                &config,
            )
            .unwrap();
        assert_eq!(result, "𝍶𝍴");

        let result = fmt
            .format(
                &Value::Number {
                    val: 9.0,
                    unit: Unit::None,
                },
                &config,
            )
            .unwrap();
        assert_eq!(result, "𝍶𝍵");

        let result = fmt
            .format(
                &Value::Number {
                    val: 10.0,
                    unit: Unit::None,
                },
                &config,
            )
            .unwrap();
        assert_eq!(result, "𝍶𝍶");
    }

    #[test]
    fn tally_western_tally_negative() {
        let fmt = new_fmt!(tally, style: western_tally).unwrap();
        let config = SharedConfig::default();

        let result = fmt.format(
            &Value::Number {
                val: -1.0,
                unit: Unit::None,
            },
            &config,
        );
        assert!(result.is_err());
    }

    #[test]
    fn tally_western_tally_positive() {
        let fmt = new_fmt!(tally, style: western_tally).unwrap();
        let config = SharedConfig::default();

        let result = fmt
            .format(
                &Value::Number {
                    val: 0.0,
                    unit: Unit::None,
                },
                &config,
            )
            .unwrap();
        assert_eq!(result, "");

        let result = fmt
            .format(
                &Value::Number {
                    val: 1.0,
                    unit: Unit::None,
                },
                &config,
            )
            .unwrap();
        assert_eq!(result, "𝍷");

        let result = fmt
            .format(
                &Value::Number {
                    val: 2.0,
                    unit: Unit::None,
                },
                &config,
            )
            .unwrap();
        assert_eq!(result, "𝍷𝍷");

        let result = fmt
            .format(
                &Value::Number {
                    val: 3.0,
                    unit: Unit::None,
                },
                &config,
            )
            .unwrap();
        assert_eq!(result, "𝍷𝍷𝍷");

        let result = fmt
            .format(
                &Value::Number {
                    val: 4.0,
                    unit: Unit::None,
                },
                &config,
            )
            .unwrap();
        assert_eq!(result, "𝍷𝍷𝍷𝍷");

        let result = fmt
            .format(
                &Value::Number {
                    val: 5.0,
                    unit: Unit::None,
                },
                &config,
            )
            .unwrap();
        assert_eq!(result, "𝍸");

        let result = fmt
            .format(
                &Value::Number {
                    val: 6.0,
                    unit: Unit::None,
                },
                &config,
            )
            .unwrap();
        assert_eq!(result, "𝍸𝍷");
    }

    #[test]
    fn tally_western_tally_ungrouped_negative() {
        let fmt = new_fmt!(tally, style: western_tally_ungrouped).unwrap();
        let config = SharedConfig::default();

        let result = fmt.format(
            &Value::Number {
                val: -1.0,
                unit: Unit::None,
            },
            &config,
        );
        assert!(result.is_err());
    }

    #[test]
    fn tally_western_tally_ungrouped_positive() {
        let fmt = new_fmt!(tally, style: western_tally_ungrouped).unwrap();
        let config = SharedConfig::default();

        let result = fmt
            .format(
                &Value::Number {
                    val: 0.0,
                    unit: Unit::None,
                },
                &config,
            )
            .unwrap();
        assert_eq!(result, "");

        let result = fmt
            .format(
                &Value::Number {
                    val: 1.0,
                    unit: Unit::None,
                },
                &config,
            )
            .unwrap();
        assert_eq!(result, "𝍷");

        let result = fmt
            .format(
                &Value::Number {
                    val: 2.0,
                    unit: Unit::None,
                },
                &config,
            )
            .unwrap();
        assert_eq!(result, "𝍷𝍷");

        let result = fmt
            .format(
                &Value::Number {
                    val: 3.0,
                    unit: Unit::None,
                },
                &config,
            )
            .unwrap();
        assert_eq!(result, "𝍷𝍷𝍷");

        let result = fmt
            .format(
                &Value::Number {
                    val: 4.0,
                    unit: Unit::None,
                },
                &config,
            )
            .unwrap();
        assert_eq!(result, "𝍷𝍷𝍷𝍷");

        let result = fmt
            .format(
                &Value::Number {
                    val: 5.0,
                    unit: Unit::None,
                },
                &config,
            )
            .unwrap();
        assert_eq!(result, "𝍷𝍷𝍷𝍷𝍷");

        let result = fmt
            .format(
                &Value::Number {
                    val: 6.0,
                    unit: Unit::None,
                },
                &config,
            )
            .unwrap();
        assert_eq!(result, "𝍷𝍷𝍷𝍷𝍷𝍷");
    }
}

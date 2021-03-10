use crate::errors::*;

use super::prefix::Prefix;
use super::unit::Unit;
use super::Variable;

#[derive(Debug, Clone)]
pub struct Value {
    unit: Unit,
    min_width: usize,
    icon: Option<String>,
    value: InternalValue,
}

#[derive(Debug, Clone)]
enum InternalValue {
    Text(String),
    Integer(i64),
    Float(f64),
}

fn format_number(
    raw_value: f64,
    min_width: usize,
    min_prefix: Prefix,
    pad_with: char,
) -> Result<String> {
    let min_exp_level = match min_prefix {
        Prefix::Tera => 4,
        Prefix::Giga => 3,
        Prefix::Mega => 2,
        Prefix::Kilo => 1,
        Prefix::One => 0,
        Prefix::Milli => -1,
        Prefix::Micro => -2,
        Prefix::Nano => -3,
        x => {
            return Err(ConfigurationError(
                "incorrect `min_prefix`".to_string(),
                format!("prefix '{}' cannot be used to format a number", x),
            ))
        }
    };

    let exp_level = (raw_value.log10().div_euclid(3.) as i32).clamp(min_exp_level, 4);
    let value = raw_value / (10f64).powi(exp_level * 3);

    let prefix = match exp_level {
        4 => Prefix::Tera,
        3 => Prefix::Giga,
        2 => Prefix::Mega,
        1 => Prefix::Kilo,
        0 => Prefix::One,
        -1 => Prefix::Milli,
        -2 => Prefix::Micro,
        _ => Prefix::Nano,
    };

    // The length of the integer part of a number
    let digits = (value.log10().floor() + 1.0).max(1.0) as isize;
    // How many characters is left for "." and the fractional part?
    Ok(match min_width as isize - digits {
        // No characters left
        x if x <= 0 => format!("{:.0}{}", value, prefix),
        // Only one character -> pad text to the right
        x if x == 1 => format!("{}{:.0}{}", pad_with, value, prefix),
        // There is space for fractional part
        rest => format!("{:.*}{}", (rest as usize) - 1, value, prefix),
    })
}

// Like format_number, but for bytes
fn format_bytes(
    raw_value: f64,
    min_width: usize,
    min_prefix: Prefix,
    pad_with: char,
) -> Result<String> {
    let min_exp_level = match min_prefix {
        Prefix::Ti => 4,
        Prefix::Gi => 3,
        Prefix::Mi => 3,
        Prefix::Ki => 1,
        Prefix::One => 0,
        x => {
            return Err(ConfigurationError(
                "incorrect `min_prefix`".to_string(),
                format!("prefix '{}' cannot be used to format byte value", x),
            ))
        }
    };

    let exp_level = (raw_value.log2().div_euclid(10.) as i32).clamp(min_exp_level, 4);
    let value = raw_value / (2f64).powi(exp_level * 10);

    let prefix = match exp_level {
        4 => Prefix::Ti,
        3 => Prefix::Gi,
        2 => Prefix::Mi,
        1 => Prefix::Ki,
        _ => Prefix::One,
    };

    // The length of the integer part of a number
    let digits = (value.log10().floor() + 1.0).max(1.0) as isize;
    // How many characters is left for "." and the fractional part?
    Ok(match min_width as isize - digits {
        // No characters left
        x if x <= 0 => format!("{:.0}{}", value, prefix),
        // Only one character -> pad text to the right
        x if x == 1 => format!("{}{:.0}{}", pad_with, value, prefix),
        // There is space for fractional part
        rest => format!("{:.*}{}", (rest as usize) - 1, value, prefix),
    })
}

fn format_bar(value: f64, length: usize) -> String {
    let value = value.clamp(0., 1.);
    let chars_to_fill = value * length as f64;
    (0..length)
        .map(|i| {
            let printed_chars = i as f64;
            let val = (chars_to_fill - printed_chars).clamp(0., 1.) * 8.;
            match val as usize {
                0 => ' ',
                1 => '\u{258f}',
                2 => '\u{258e}',
                3 => '\u{258d}',
                4 => '\u{258c}',
                5 => '\u{258b}',
                6 => '\u{258a}',
                7 => '\u{2589}',
                _ => '\u{2588}',
            }
        })
        .collect()
}

impl Value {
    // Constuctors
    pub fn from_string(text: String) -> Self {
        Self {
            icon: None,
            min_width: 0,
            unit: Unit::None,
            value: InternalValue::Text(text),
        }
    }
    pub fn from_integer(value: i64) -> Self {
        Self {
            icon: None,
            min_width: 2,
            unit: Unit::None,
            value: InternalValue::Integer(value),
        }
    }
    pub fn from_float(value: f64) -> Self {
        Self {
            icon: None,
            min_width: 3,
            unit: Unit::None,
            value: InternalValue::Float(value),
        }
    }

    // Set options
    pub fn icon(mut self, icon: String) -> Self {
        self.icon = Some(icon);
        self
    }
    //pub fn min_width(mut self, min_width: usize) -> Self {
    //self.min_width = min_width;
    //self
    //}

    // Units
    pub fn degrees(mut self) -> Self {
        self.unit = Unit::Degrees;
        self
    }
    pub fn percents(mut self) -> Self {
        self.unit = Unit::Percents;
        self
    }
    pub fn bits_per_second(mut self) -> Self {
        self.unit = Unit::BitsPerSecond;
        self
    }
    pub fn bytes_per_second(mut self) -> Self {
        self.unit = Unit::BytesPerSecond;
        self
    }
    pub fn seconds(mut self) -> Self {
        self.unit = Unit::Seconds;
        self
    }
    pub fn watts(mut self) -> Self {
        self.unit = Unit::Watts;
        self
    }
    pub fn hertz(mut self) -> Self {
        self.unit = Unit::Hertz;
        self
    }
    pub fn bytes(mut self) -> Self {
        self.unit = Unit::Bytes;
        self
    }

    //TODO impl Display
    pub fn format(&self, var: &Variable) -> Result<String> {
        let min_width = var.min_width.unwrap_or(self.min_width);
        let pad_with = var.pad_with.unwrap_or(' ');
        let unit = var.unit.unwrap_or(self.unit);

        // Draw the bar instead of usual formatting if `bar_max_value` is set
        // (olny for integers and floats)
        if let Some(bar_max_value) = var.bar_max_value {
            match self.value {
                InternalValue::Integer(i) => {
                    return Ok(format_bar(i as f64 / bar_max_value, min_width))
                }
                InternalValue::Float(f) => return Ok(format_bar(f / bar_max_value, min_width)),
                _ => (),
            }
        }

        let value = match self.value {
            InternalValue::Text(ref text) => {
                let mut text = text.clone();
                let text_len = text.len();
                for _ in text_len..min_width {
                    text.push(pad_with);
                }
                if let Some(max_width) = var.max_width {
                    text.truncate(max_width);
                }
                text
            }
            InternalValue::Integer(value) => {
                let value = (value as f64 * self.unit.convert(unit)?) as i64;

                let text = value.to_string();
                let mut retval = String::new();
                let text_len = text.len();
                for _ in text_len..min_width {
                    retval.push(pad_with);
                }
                retval.push_str(&text);
                retval
            }
            InternalValue::Float(value) => {
                let value = value * self.unit.convert(unit)?;

                if unit == Unit::Bytes
                    || unit == Unit::BytesPerSecond
                    || unit == Unit::BitsPerSecond
                {
                    format_bytes(
                        value,
                        min_width,
                        var.min_prefix.unwrap_or(Prefix::One),
                        pad_with,
                    )?
                } else {
                    format_number(
                        value,
                        min_width,
                        var.min_prefix.unwrap_or(Prefix::Nano),
                        pad_with,
                    )?
                }
            }
        };
        Ok(if let Some(ref icon) = self.icon {
            format!("{}{}{}", icon, value, unit)
        } else {
            format!("{}{}", value, unit)
        })
    }
}
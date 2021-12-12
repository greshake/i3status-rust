use crate::errors::*;

use super::placeholder::{MinPrefixConfig, Placeholder};
use super::prefix::Prefix;
use super::unit::Unit;

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
    Boolean(bool),
}

fn format_number(
    raw_value: f64,
    min_width: usize,
    min_prefix_config: MinPrefixConfig,
    unit: Unit,
    pad_with: char,
) -> String {
    let min_prefix = min_prefix_config.value.unwrap_or(Prefix::Nano);
    let is_byte = unit.is_byte();

    let mut min_exp_level = match min_prefix {
        Prefix::Tera => 4,
        Prefix::Giga => 3,
        Prefix::Mega => 2,
        Prefix::Kilo => 1,
        Prefix::One => 0,
        Prefix::Milli => -1,
        Prefix::Micro => -2,
        Prefix::Nano => -3,
    };

    if is_byte {
        min_exp_level = min_exp_level.max(0);
    }

    let (mut value, mut prefix) = if !is_byte {
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
        (value, prefix)
    } else {
        let exp_level = (raw_value.log2().div_euclid(10.) as i32).clamp(min_exp_level, 4);
        let value = raw_value / (2f64).powi(exp_level * 10);

        let prefix = match exp_level {
            4 => Prefix::Tera,
            3 => Prefix::Giga,
            2 => Prefix::Mega,
            1 => Prefix::Kilo,
            _ => Prefix::One,
        };
        (value, prefix)
    };

    if unit == Unit::Percents || unit == Unit::None {
        value = raw_value;
        prefix = Prefix::One;
    }

    // Apply prefix' configuration
    let mut prefix_str = if min_prefix_config.space {
        " ".to_string()
    } else {
        String::new()
    };
    if !min_prefix_config.hidden {
        prefix_str.push_str(&prefix.to_string());
    }

    // The length of the integer part of a number
    let digits = (value.log10().floor() + 1.0).max(1.0) as isize;
    // How many characters is left for "." and the fractional part?
    match min_width as isize - digits {
        // No characters left
        x if x <= 0 => format!("{:.0}{}", value, prefix_str),
        // Only one character -> pad text to the right
        x if x == 1 => format!("{}{:.0}{}", pad_with, value, prefix_str),
        // There is space for fractional part
        rest => format!("{:.*}{}", (rest as usize) - 1, value, prefix_str),
    }
}

fn format_bar(value: f64, length: usize) -> String {
    let value = value.clamp(0., 1.);
    let chars_to_fill = value * length as f64;
    (0..length)
        .map(|i| {
            let printed_chars = i as f64;
            let val = (chars_to_fill - printed_chars).clamp(0., 1.) * 8.;
            match val as usize {
                //TODO make those characters configurable?
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
    // Constructors
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
    pub fn from_boolean(value: bool) -> Self {
        Self {
            icon: None,
            min_width: 2,
            unit: Unit::None,
            value: InternalValue::Boolean(value),
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
    pub fn bytes(mut self) -> Self {
        self.unit = Unit::Bytes;
        self
    }
    pub fn bits(mut self) -> Self {
        self.unit = Unit::Bits;
        self
    }
    pub fn degrees(mut self) -> Self {
        self.unit = Unit::Degrees;
        self
    }
    pub fn percents(mut self) -> Self {
        self.unit = Unit::Percents;
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

    pub fn format(&self, var: &Placeholder) -> Result<String> {
        // Get user-specified min_width and pad_with values. Use defaults instead
        let min_width = var.min_width.min_width.unwrap_or(self.min_width);
        let pad_with = var.min_width.pad_with;
        // Apply unit override
        let unit = var.unit.unit.unwrap_or(self.unit);

        // Draw the bar instead of usual formatting if `bar_max_value` is set
        // (only for integers and floats)
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
                // Format text value. First pad it to the left with `pad_with` symbol. Then apply
                // `max_width` option.
                let mut text = text.clone();
                for _ in (text.chars().count())..min_width {
                    text.push(pad_with);
                }
                if let Some(max_width) = var.max_width {
                    for _ in 0..(text.chars().count() as isize - max_width as isize) {
                        text.pop();
                    }
                }
                text
            }
            InternalValue::Integer(value) => {
                // Convert the value
                // TODO better conversion mechanism
                let value = (value as f64 * self.unit.convert(unit)?) as i64;

                // Pad the resulting string to the right
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
                // Convert the value
                // TODO better conversion mechanism
                let value = value * self.unit.convert(unit)?;

                // Apply engineering notation (Float-only)
                format_number(value, min_width, var.min_prefix, unit, pad_with)
            }
            InternalValue::Boolean(value) => match value {
                true => String::from("T"),
                false => String::from("F"),
            },
        };

        // We prepend the resulting string with the icon if it is set
        let icon_str = self.icon.as_deref().unwrap_or("");

        // Hide the unit if a corresponding option is set
        let unit = if var.unit.hidden {
            String::new()
        } else {
            unit.to_string()
        };

        Ok(format!("{}{}{}", icon_str, value, unit))
    }
}

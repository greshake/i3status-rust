use std::convert::TryInto;

use super::prefix::Prefix;
use super::unit::Unit;
use crate::errors::*;

const MIN_WIDTH_TOKEN: char = ':';
const MAX_WIDTH_TOKEN: char = '^';
const MIN_SUFFIX_TOKEN: char = ';';
const UNIT_TOKEN: char = '*';
const BAR_MAX_VAL_TOKEN: char = '#';

#[derive(Debug, Clone, PartialEq)]
pub struct Placeholder {
    pub name: String,
    pub min_width: Option<usize>,
    pub max_width: Option<usize>,
    pub pad_with: Option<char>,
    pub min_prefix: Option<Prefix>,
    pub unit: Option<Unit>,
    pub unit_hidden: bool,
    pub bar_max_value: Option<f64>,
}

fn unexpected_token<T>(token: char) -> Result<T> {
    Err(ConfigurationError(
        format!(
            "failed to parse formatting string: unexpected token '{}'",
            token
        ),
        String::new(),
    ))
}

impl TryInto<Placeholder> for &str {
    type Error = crate::errors::Error;

    fn try_into(self) -> Result<Placeholder> {
        let mut var_buf = String::new();
        let mut min_width_buf = String::new();
        let mut max_width_buf = String::new();
        let mut min_prefix_buf = String::new();
        let mut unit_buf = String::new();
        let mut bar_max_value_buf = String::new();

        let mut current_buf = &mut var_buf;

        for c in self.chars() {
            match c {
                MIN_WIDTH_TOKEN => {
                    if !min_width_buf.is_empty() {
                        return unexpected_token(c);
                    }
                    current_buf = &mut min_width_buf;
                }
                MAX_WIDTH_TOKEN => {
                    if !min_width_buf.is_empty() {
                        return unexpected_token(c);
                    }
                    current_buf = &mut max_width_buf;
                }
                MIN_SUFFIX_TOKEN => {
                    if !min_prefix_buf.is_empty() {
                        return unexpected_token(c);
                    }
                    current_buf = &mut min_prefix_buf;
                }
                UNIT_TOKEN => {
                    if !unit_buf.is_empty() {
                        return unexpected_token(c);
                    }
                    current_buf = &mut unit_buf;
                }
                BAR_MAX_VAL_TOKEN => {
                    if !bar_max_value_buf.is_empty() {
                        return unexpected_token(c);
                    }
                    current_buf = &mut bar_max_value_buf;
                }
                x => current_buf.push(x),
            }
        }

        // Parse padding
        let (min_width, pad_with) =
            if min_width_buf.is_empty() {
                (None, None)
            } else if let ("0", "") = min_width_buf.split_at(1) {
                (Some(0), None)
            } else if let ("0", min_width) = min_width_buf.split_at(1) {
                (
                    Some(min_width.parse().configuration_error(&format!(
                        "failed to parse min_width '{}'",
                        min_width
                    ))?),
                    Some('0'),
                )
            } else {
                (
                    Some(min_width_buf.parse().configuration_error(&format!(
                        "failed to parse min_width '{}'",
                        min_width_buf
                    ))?),
                    None,
                )
            };
        // Parse max_width
        let max_width =
            if max_width_buf.is_empty() {
                None
            } else {
                Some(max_width_buf.parse().configuration_error(&format!(
                    "failed to parse max_width '{}'",
                    max_width_buf
                ))?)
            };
        // Parse min_prefix
        let min_prefix = if min_prefix_buf.is_empty() {
            None
        } else {
            Some(min_prefix_buf.as_str().try_into()?)
        };
        // Parse unit
        let (unit, unit_hidden) = if unit_buf.is_empty() {
            (None, false)
        } else if let ("_", unit) = unit_buf.split_at(1) {
            (Some(unit.try_into()?), true)
        } else {
            (Some(unit_buf.as_str().try_into()?), false)
        };
        // Parse bar_max_value
        let bar_max_value = if bar_max_value_buf.is_empty() {
            None
        } else {
            Some(bar_max_value_buf.parse().configuration_error(&format!(
                "failed to parse bar_max_value '{}'",
                bar_max_value_buf
            ))?)
        };

        Ok(Placeholder {
            name: var_buf,
            min_width,
            max_width,
            pad_with,
            min_prefix,
            unit,
            unit_hidden,
            bar_max_value,
        })
    }
}

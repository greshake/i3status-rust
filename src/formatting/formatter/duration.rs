use std::cmp::min;

use super::*;

const UNIT_COUNT: usize = 7;
const UNITS: [&str; UNIT_COUNT] = ["y", "w", "d", "h", "m", "s", "ms"];
const UNIT_CONVERSION_RATES: [u128; UNIT_COUNT] = [52, 7, 24, 60, 60, 1000, 1];
const UNIT_PAD_WIDTHS: [usize; UNIT_COUNT] = [1, 2, 1, 2, 2, 2, 3];

pub const DEFAULT_DURATION_FORMATTER: DurationFormatter = DurationFormatter {
    max_unit_index: 0,
    min_unit_index: 5,
    units: 2,
    round_up: true,
    prefix_has_space: false,
    pad_with: DEFAULT_NUMBER_PAD_WITH,
    show_leading_units_if_zero: true,
};

#[derive(Debug, Default)]
pub struct DurationFormatter {
    max_unit_index: usize,
    min_unit_index: usize,
    units: usize,
    round_up: bool,
    prefix_has_space: bool,
    pad_with: char,
    show_leading_units_if_zero: bool,
}

impl DurationFormatter {
    pub(super) fn from_args(args: &[Arg]) -> Result<Self> {
        let mut max_unit = "y";
        let mut min_unit = "s";
        let mut units: Option<usize> = None;
        let mut round_up = true;
        let mut prefix_has_space = false;
        let mut pad_with = DEFAULT_NUMBER_PAD_WITH;
        let mut show_leading_units_if_zero = true;
        for arg in args {
            match arg.key {
                "max_unit" => {
                    max_unit = arg.val;
                }
                "min_unit" => {
                    min_unit = arg.val;
                }
                "units" => {
                    units = Some(
                        arg.val
                            .parse()
                            .ok()
                            .error("units must be a positive integer")?,
                    );
                }
                "round_up" => {
                    round_up = arg
                        .val
                        .parse()
                        .ok()
                        .error("round_up must be true or false")?;
                }
                "prefix_space" => {
                    prefix_has_space = arg
                        .val
                        .parse()
                        .ok()
                        .error("prefix_space must be true or false")?;
                }
                "pad_with" => {
                    pad_with = arg
                        .val
                        .parse()
                        .error("pad_with must be a single character")?;
                }
                "show_leading_units_if_zero" => {
                    show_leading_units_if_zero =
                        arg.val.parse().ok().error("units must be true or false")?;
                }
                _ => return Err(Error::new(format!("Unexpected argument {:?}", arg.key))),
            }
        }

        let max_unit_index = UNITS
            .iter()
            .position(|&x| x == max_unit)
            .error("max_unit must be one of \"y\", \"w\", \"d\", \"h\", \"m\", \"s\", or \"ms\"")?;

        let min_unit_index = UNITS
            .iter()
            .position(|&x| x == min_unit)
            .error("min_unit must be one of \"y\", \"w\", \"d\", \"h\", \"m\", \"s\", or \"ms\"")?;

        // UNITS are sorted largest to smallest
        if min_unit_index < max_unit_index {
            return Err(Error::new(format!(
                "min_unit ({}) must be smaller than or equal to max_unit({})",
                min_unit, max_unit,
            )));
        }

        let units_upper_bound = min_unit_index - max_unit_index + 1;
        let units = units.unwrap_or_else(|| min(units_upper_bound, 2));

        if units > units_upper_bound {
            return Err(Error::new(format!(
                "there aren't {} units between min_unit ({}) and max_unit({})",
                units, min_unit, max_unit,
            )));
        }

        Ok(Self {
            max_unit_index,
            min_unit_index,
            units,
            round_up,
            prefix_has_space,
            pad_with,
            show_leading_units_if_zero,
        })
    }
}

impl Formatter for DurationFormatter {
    fn format(&self, val: &Value, _config: &SharedConfig) -> Result<String, FormatError> {
        match val {
            Value::Duration(duration) => {
                let mut ms = duration.as_millis();
                if self.round_up {
                    ms += UNIT_CONVERSION_RATES[self.min_unit_index..]
                        .iter()
                        .product::<u128>()
                        - 1;
                }

                let mut v = Vec::new();
                let mut div = UNIT_CONVERSION_RATES[self.max_unit_index..=self.min_unit_index]
                    .iter()
                    .product::<u128>();
                for rate in &UNIT_CONVERSION_RATES[self.max_unit_index..=self.min_unit_index] {
                    v.push(ms / div);
                    ms %= div;
                    div /= rate;
                }

                let value_unit_seporator = if self.prefix_has_space { " " } else { "" };

                Ok(itertools::join(
                    v.iter()
                        .enumerate()
                        // Zip up the values of time with the matching units of time
                        .zip(UNITS[self.max_unit_index..=self.min_unit_index].iter())
                        // Skip wile the value is zero, unless we want to display the leading units of time with value of zero.
                        // For example we want to have a minimum unit of seconds but to always show two values we could have:
                        // " 0m 15s"
                        .skip_while(|((i, value), _)| {
                            **value == 0
                                && (!self.show_leading_units_if_zero
                                    || *i + self.max_unit_index
                                        != self.min_unit_index - self.units + 1)
                        })
                        .take(self.units)
                        .map(|((i, value), unit)| {
                            let pad_width = UNIT_PAD_WIDTHS[i + self.max_unit_index];
                            let mut value_str = value.to_string();
                            while value_str.len() < pad_width {
                                value_str.insert(0, self.pad_with);
                            }
                            format!("{}{}{}", value_str, value_unit_seporator, unit)
                        }),
                    " ",
                ))
            }
            other => Err(FormatError::IncompatibleFormatter {
                ty: other.type_name(),
                fmt: "duration",
            }),
        }
    }
}

use std::cmp::min;

use super::*;

const UNIT_COUNT: usize = 7;
const UNITS: [&str; UNIT_COUNT] = ["y", "w", "d", "h", "m", "s", "ms"];
const UNIT_CONVERSION_RATES: [u128; UNIT_COUNT] = [
    31_556_952_000, // Based on there being 365.2425 days/year
    604_800_000,
    86_400_000,
    3_600_000,
    60_000,
    1_000,
    1,
];
const UNIT_PAD_WIDTHS: [usize; UNIT_COUNT] = [1, 2, 1, 2, 2, 2, 3];

pub const DEFAULT_DURATION_FORMATTER: DurationFormatter = DurationFormatter {
    hms: false,
    max_unit_index: 0,
    min_unit_index: 5,
    units: 2,
    round_up: true,
    unit_has_space: false,
    pad_with: DEFAULT_NUMBER_PAD_WITH,
    show_leading_units_if_zero: true,
};

#[derive(Debug, Default)]
pub struct DurationFormatter {
    hms: bool,
    max_unit_index: usize,
    min_unit_index: usize,
    units: usize,
    round_up: bool,
    unit_has_space: bool,
    pad_with: char,
    show_leading_units_if_zero: bool,
}

impl DurationFormatter {
    pub(super) fn from_args(args: &[Arg]) -> Result<Self> {
        let mut hms = false;
        let mut max_unit = None;
        let mut min_unit = "s";
        let mut units: Option<usize> = None;
        let mut round_up = true;
        let mut unit_has_space = false;
        let mut pad_with = None;
        let mut show_leading_units_if_zero = true;
        for arg in args {
            match arg.key {
                "hms" => {
                    hms = arg.val.parse().ok().error("hms must be true or false")?;
                }
                "max_unit" => {
                    max_unit = Some(arg.val);
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
                "unit_space" => {
                    unit_has_space = arg
                        .val
                        .parse()
                        .ok()
                        .error("unit_space must be true or false")?;
                }
                "pad_with" => {
                    pad_with = Some(if arg.val.is_empty() {
                        '\u{200B}' // zero-width space
                    } else {
                        arg.val
                            .parse()
                            .error("pad_with must be a single character")?
                    });
                }
                "show_leading_units_if_zero" => {
                    show_leading_units_if_zero =
                        arg.val.parse().ok().error("units must be true or false")?;
                }

                _ => return Err(Error::new(format!("Unexpected argument {:?}", arg.key))),
            }
        }

        if hms && unit_has_space {
            return Err(Error::new(
                "When hms is enabled prefix_has_space should not be true",
            ));
        }

        let max_unit = max_unit.unwrap_or(if hms { "h" } else { "y" });
        let pad_with = pad_with.unwrap_or(if hms { '0' } else { DEFAULT_NUMBER_PAD_WITH });

        let max_unit_index = UNITS
            .iter()
            .position(|&x| x == max_unit)
            .error("max_unit must be one of \"y\", \"w\", \"d\", \"h\", \"m\", \"s\", or \"ms\"")?;

        let min_unit_index = UNITS
            .iter()
            .position(|&x| x == min_unit)
            .error("min_unit must be one of \"y\", \"w\", \"d\", \"h\", \"m\", \"s\", or \"ms\"")?;

        if hms && max_unit_index < 3 {
            return Err(Error::new(
                "When hms is enabled the max unit must be h,m,s,ms",
            ));
        }

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
            hms,
            max_unit_index,
            min_unit_index,
            units,
            round_up,
            unit_has_space,
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
                    ms += UNIT_CONVERSION_RATES[self.min_unit_index] - 1;
                }

                let mut v = Vec::new();
                for div in &UNIT_CONVERSION_RATES[self.max_unit_index..=self.min_unit_index] {
                    v.push(ms / div);
                    ms %= div;
                }

                let mut first_entry = true;
                let mut result = String::new();
                v.iter()
                    .enumerate()
                    // Skip wile the value is zero, unless we want to display the leading units of time with value of zero.
                    // For example we want to have a minimum unit of seconds but to always show two values we could have:
                    // " 0m 15s"
                    .skip_while(|&(i, &value)| {
                        value == 0
                            && (!self.show_leading_units_if_zero
                                || i + self.max_unit_index != self.min_unit_index + 1 - self.units)
                    })
                    .take(self.units)
                    .for_each(|(i, &value)| {
                        let index = i + self.max_unit_index;
                        // No separator before the first entry
                        if !first_entry {
                            if self.hms {
                                // Separator between s and ms should be a '.'
                                if index == 6 {
                                    result.push('.');
                                } else {
                                    result.push(':');
                                }
                            } else {
                                result.push(' ');
                            }
                        } else {
                            first_entry = false;
                        }

                        // Pad the value
                        let pad_width = UNIT_PAD_WIDTHS[index];
                        let value_str = value.to_string();
                        for _ in value_str.len()..pad_width {
                            result.push(self.pad_with);
                        }
                        result.push_str(&value_str);

                        // No units in hms mode
                        if !self.hms {
                            if self.unit_has_space {
                                result.push(' ');
                            }
                            result.push_str(UNITS[index]);
                        }
                    });

                Ok(result)
            }
            other => Err(FormatError::IncompatibleFormatter {
                ty: other.type_name(),
                fmt: "duration",
            }),
        }
    }
}

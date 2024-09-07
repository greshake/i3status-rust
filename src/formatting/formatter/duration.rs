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
    leading_zeroes: true,
};

#[derive(Debug, Default)]
pub struct DurationFormatter {
    hms: bool,
    max_unit_index: usize,
    min_unit_index: usize,
    units: usize,
    round_up: bool,
    unit_has_space: bool,
    pad_with: PadWith,
    leading_zeroes: bool,
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
        let mut leading_zeroes = true;
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
                    if arg.val.graphemes(true).count() < 2 {
                        pad_with = Some(Cow::Owned(arg.val.into()));
                    } else {
                        return Err(Error::new(
                            "pad_with must be an empty string or a single character",
                        ));
                    };
                }
                "leading_zeroes" => {
                    leading_zeroes = arg.val.parse().ok().error("units must be true or false")?;
                }

                _ => return Err(Error::new(format!("Unexpected argument {:?}", arg.key))),
            }
        }

        if hms && unit_has_space {
            return Err(Error::new(
                "When hms is enabled unit_space should not be true",
            ));
        }

        let max_unit = max_unit.unwrap_or(if hms { "h" } else { "y" });
        let pad_with = pad_with.unwrap_or(if hms {
            Cow::Borrowed("0")
        } else {
            DEFAULT_NUMBER_PAD_WITH
        });

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
                "min_unit({}) must be smaller than or equal to max_unit({})",
                min_unit, max_unit,
            )));
        }

        let units_upper_bound = min_unit_index - max_unit_index + 1;
        let units = units.unwrap_or_else(|| min(units_upper_bound, 2));

        if units > units_upper_bound {
            return Err(Error::new(format!(
                "there aren't {} units between min_unit({}) and max_unit({})",
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
            leading_zeroes,
        })
    }

    fn get_time_parts(&self, mut ms: u128) -> Vec<(usize, u128)> {
        let mut should_push = false;
        // A Vec of the unit index and value pairs
        let mut v = Vec::with_capacity(self.units);
        for (i, div) in UNIT_CONVERSION_RATES[self.max_unit_index..=self.min_unit_index]
            .iter()
            .enumerate()
        {
            // Offset i by the offset used to slice UNIT_CONVERSION_RATES
            let index = i + self.max_unit_index;
            let value = ms / div;

            // Only add the non-zero, unless we want to display the leading units of time with value of zero.
            // For example we want to have a minimum unit of seconds but to always show two values we could have:
            // " 0m 15s"
            if !should_push {
                should_push = value != 0
                    || (self.leading_zeroes && index >= self.min_unit_index + 1 - self.units);
            }

            if should_push {
                v.push((index, value));
                // We have the right number of values/units
                if v.len() == self.units {
                    break;
                }
            }
            ms %= div;
        }

        v
    }
}

impl Formatter for DurationFormatter {
    fn format(&self, val: &Value, _config: &SharedConfig) -> Result<String, FormatError> {
        match val {
            Value::Duration(duration) => {
                let mut v = self.get_time_parts(duration.as_millis());

                if self.round_up {
                    // Get the index for which unit we should round up to
                    let i = v.last().map_or(self.min_unit_index, |&(i, _)| i);
                    v = self.get_time_parts(duration.as_millis() + UNIT_CONVERSION_RATES[i] - 1);
                }

                let mut first_entry = true;
                let mut result = String::new();
                for (i, value) in v {
                    // No separator before the first entry
                    if !first_entry {
                        if self.hms {
                            // Separator between s and ms should be a '.'
                            if i == 6 {
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
                    let value_str = value.to_string();
                    for _ in value_str.len()..UNIT_PAD_WIDTHS[i] {
                        result.push_str(&self.pad_with);
                    }
                    result.push_str(&value_str);

                    // No units in hms mode
                    if !self.hms {
                        if self.unit_has_space {
                            result.push(' ');
                        }
                        result.push_str(UNITS[i]);
                    }
                }

                Ok(result)
            }
            other => Err(FormatError::IncompatibleFormatter {
                ty: other.type_name(),
                fmt: "duration",
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! dur {
        ($($key:ident : $value:expr),*) => {{
            let mut ms = 0;
            $(
            let unit = stringify!($key);
            ms += $value
                * (UNIT_CONVERSION_RATES[UNITS
                    .iter()
                    .position(|&x| x == unit)
                    .expect("unit must be one of \"y\", \"w\", \"d\", \"h\", \"m\", \"s\", or \"ms\"")]
                    as u64);
            )*
           Value::Duration(std::time::Duration::from_millis(ms))
        }};
    }

    #[test]
    fn dur_default_single_unit() {
        let config = SharedConfig::default();
        let fmt = new_fmt!(dur).unwrap();

        let result = fmt.format(&dur!(y:1), &config).unwrap();
        assert_eq!(result, "1y  0w");

        let result = fmt.format(&dur!(w:1), &config).unwrap();
        assert_eq!(result, " 1w 0d");

        let result = fmt.format(&dur!(d:1), &config).unwrap();
        assert_eq!(result, "1d  0h");

        let result = fmt.format(&dur!(h:1), &config).unwrap();
        assert_eq!(result, " 1h  0m");

        let result = fmt.format(&dur!(m:1), &config).unwrap();
        assert_eq!(result, " 1m  0s");

        let result = fmt.format(&dur!(s:1), &config).unwrap();
        assert_eq!(result, " 0m  1s");

        //This is rounded to 1s since min_unit is 's' and round_up is true
        let result = fmt.format(&dur!(ms:1), &config).unwrap();
        assert_eq!(result, " 0m  1s");
    }

    #[test]
    fn dur_default_consecutive_units() {
        let config = SharedConfig::default();
        let fmt = new_fmt!(dur).unwrap();

        let result = fmt.format(&dur!(y:1, w:2), &config).unwrap();
        assert_eq!(result, "1y  2w");

        let result = fmt.format(&dur!(w:1, d:2), &config).unwrap();
        assert_eq!(result, " 1w 2d");

        let result = fmt.format(&dur!(d:1, h:2), &config).unwrap();
        assert_eq!(result, "1d  2h");

        let result = fmt.format(&dur!(h:1, m:2), &config).unwrap();
        assert_eq!(result, " 1h  2m");

        let result = fmt.format(&dur!(m:1, s:2), &config).unwrap();
        assert_eq!(result, " 1m  2s");

        //This is rounded to 2s since min_unit is 's' and round_up is true
        let result = fmt.format(&dur!(s:1, ms:2), &config).unwrap();
        assert_eq!(result, " 0m  2s");
    }

    #[test]
    fn dur_hms_no_ms() {
        let config = SharedConfig::default();
        let fmt = new_fmt!(dur, hms:true, min_unit:s).unwrap();

        let result = fmt.format(&dur!(d:1, h:2), &config).unwrap();
        assert_eq!(result, "26:00");

        let result = fmt.format(&dur!(h:1, m:2), &config).unwrap();
        assert_eq!(result, "01:02");

        let result = fmt.format(&dur!(m:1, s:2), &config).unwrap();
        assert_eq!(result, "01:02");

        //This is rounded to 2s since min_unit is 's' and round_up is true
        let result = fmt.format(&dur!(s:1, ms:2), &config).unwrap();
        assert_eq!(result, "00:02");
    }

    #[test]
    fn dur_hms_with_ms() {
        let config = SharedConfig::default();
        let fmt = new_fmt!(dur, hms:true, min_unit:ms).unwrap();

        let result = fmt.format(&dur!(d:1, h:2), &config).unwrap();
        assert_eq!(result, "26:00");

        let result = fmt.format(&dur!(h:1, m:2), &config).unwrap();
        assert_eq!(result, "01:02");

        let result = fmt.format(&dur!(m:1, s:2), &config).unwrap();
        assert_eq!(result, "01:02");

        let result = fmt.format(&dur!(s:1, ms:2), &config).unwrap();
        assert_eq!(result, "01.002");
    }

    #[test]
    fn dur_round_up_true() {
        let config = SharedConfig::default();
        let fmt = new_fmt!(dur, round_up:true).unwrap();

        let result = fmt.format(&dur!(y:1, ms:1), &config).unwrap();
        assert_eq!(result, "1y  1w");

        let result = fmt.format(&dur!(w:1, ms:1), &config).unwrap();
        assert_eq!(result, " 1w 1d");

        let result = fmt.format(&dur!(d:1, ms:1), &config).unwrap();
        assert_eq!(result, "1d  1h");

        let result = fmt.format(&dur!(h:1, ms:1), &config).unwrap();
        assert_eq!(result, " 1h  1m");

        let result = fmt.format(&dur!(m:1, ms:1), &config).unwrap();
        assert_eq!(result, " 1m  1s");

        //This is rounded to 2s since min_unit is 's' and round_up is true
        let result = fmt.format(&dur!(s:1, ms:1), &config).unwrap();
        assert_eq!(result, " 0m  2s");
    }

    #[test]
    fn dur_units() {
        let config = SharedConfig::default();
        let val = dur!(y:1, w:2, d:3, h:4, m:5, s:6, ms:7);

        let fmt = new_fmt!(dur, round_up:false, min_unit:ms, units: 1).unwrap();
        let result = fmt.format(&val, &config).unwrap();
        assert_eq!(result, "1y");

        let fmt = new_fmt!(dur, round_up:false, min_unit:ms, units: 2).unwrap();
        let result = fmt.format(&val, &config).unwrap();
        assert_eq!(result, "1y  2w");

        let fmt = new_fmt!(dur, round_up:false, min_unit:ms, units: 3).unwrap();
        let result = fmt.format(&val, &config).unwrap();
        assert_eq!(result, "1y  2w 3d");

        let fmt = new_fmt!(dur, round_up:false, min_unit:ms, units: 4).unwrap();
        let result = fmt.format(&val, &config).unwrap();
        assert_eq!(result, "1y  2w 3d  4h");

        let fmt = new_fmt!(dur, round_up:false, min_unit:ms, units: 5).unwrap();
        let result = fmt.format(&val, &config).unwrap();
        assert_eq!(result, "1y  2w 3d  4h  5m");

        let fmt = new_fmt!(dur, round_up:false, min_unit:ms, units: 6).unwrap();
        let result = fmt.format(&val, &config).unwrap();
        assert_eq!(result, "1y  2w 3d  4h  5m  6s");

        let fmt = new_fmt!(dur, round_up:false, min_unit:ms, units: 7).unwrap();
        let result = fmt.format(&val, &config).unwrap();
        assert_eq!(result, "1y  2w 3d  4h  5m  6s   7ms");
    }

    #[test]
    fn dur_round_up_false() {
        let config = SharedConfig::default();
        let fmt = new_fmt!(dur, round_up:false).unwrap();

        let result = fmt.format(&dur!(y:1, ms:1), &config).unwrap();
        assert_eq!(result, "1y  0w");

        let result = fmt.format(&dur!(w:1, ms:1), &config).unwrap();
        assert_eq!(result, " 1w 0d");

        let result = fmt.format(&dur!(d:1, ms:1), &config).unwrap();
        assert_eq!(result, "1d  0h");

        let result = fmt.format(&dur!(h:1, ms:1), &config).unwrap();
        assert_eq!(result, " 1h  0m");

        let result = fmt.format(&dur!(m:1, ms:1), &config).unwrap();
        assert_eq!(result, " 1m  0s");

        let result = fmt.format(&dur!(s:1, ms:1), &config).unwrap();
        assert_eq!(result, " 0m  1s");

        let result = fmt.format(&dur!(ms:1), &config).unwrap();
        assert_eq!(result, " 0m  0s");
    }

    #[test]
    fn dur_invalid_config_hms_and_unit_space() {
        let fmt_err = new_fmt!(dur, hms:true, unit_space:true).unwrap_err();
        assert_eq!(
            fmt_err.message,
            Some("When hms is enabled unit_space should not be true".into())
        );
    }

    #[test]
    fn dur_invalid_config_invalid_unit() {
        let fmt_err = new_fmt!(dur, max_unit:does_not_exist).unwrap_err();
        assert_eq!(
            fmt_err.message,
            Some(
                "max_unit must be one of \"y\", \"w\", \"d\", \"h\", \"m\", \"s\", or \"ms\""
                    .into()
            )
        );

        let fmt_err = new_fmt!(dur, min_unit:does_not_exist).unwrap_err();
        assert_eq!(
            fmt_err.message,
            Some(
                "min_unit must be one of \"y\", \"w\", \"d\", \"h\", \"m\", \"s\", or \"ms\""
                    .into()
            )
        );
    }

    #[test]
    fn dur_invalid_config_hms_max_unit_too_large() {
        let fmt_err = new_fmt!(dur, max_unit:d, hms:true).unwrap_err();
        assert_eq!(
            fmt_err.message,
            Some("When hms is enabled the max unit must be h,m,s,ms".into())
        );
    }

    #[test]
    fn dur_invalid_config_min_larger_than_max() {
        let fmt = new_fmt!(dur, max_unit:h, min_unit:h);
        assert!(fmt.is_ok());

        let fmt_err = new_fmt!(dur, max_unit:h, min_unit:d).unwrap_err();
        assert_eq!(
            fmt_err.message,
            Some("min_unit(d) must be smaller than or equal to max_unit(h)".into())
        );
    }

    #[test]
    fn dur_invalid_config_too_many_units() {
        let fmt = new_fmt!(dur, max_unit:y, min_unit:s, units:6);
        assert!(fmt.is_ok());

        let fmt_err = new_fmt!(dur, max_unit:y, min_unit:s, units:7).unwrap_err();
        assert_eq!(
            fmt_err.message,
            Some("there aren't 7 units between min_unit(s) and max_unit(y)".into())
        );

        let fmt = new_fmt!(dur, max_unit:w, min_unit:s, units:5);
        assert!(fmt.is_ok());

        let fmt_err = new_fmt!(dur, max_unit:w, min_unit:s, units:6).unwrap_err();
        assert_eq!(
            fmt_err.message,
            Some("there aren't 6 units between min_unit(s) and max_unit(w)".into())
        );

        let fmt = new_fmt!(dur, max_unit:y, min_unit:ms, units:7);
        assert!(fmt.is_ok());

        let fmt_err = new_fmt!(dur, max_unit:y, min_unit:ms, units:8).unwrap_err();
        assert_eq!(
            fmt_err.message,
            Some("there aren't 8 units between min_unit(ms) and max_unit(y)".into())
        );
    }
}

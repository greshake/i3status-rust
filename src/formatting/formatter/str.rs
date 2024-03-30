use std::iter::repeat;
use std::time::Instant;

use crate::escape::CollectEscaped;

use super::*;

const DEFAULT_STR_MIN_WIDTH: usize = 0;
const DEFAULT_STR_MAX_WIDTH: usize = usize::MAX;
const DEFAULT_STR_ROT_INTERVAL: Option<f64> = None;
const DEFAULT_STR_ROT_SEP: Option<String> = None;

pub const DEFAULT_STRING_FORMATTER: StrFormatter = StrFormatter {
    min_width: DEFAULT_STR_MIN_WIDTH,
    max_width: DEFAULT_STR_MAX_WIDTH,
    rot_interval_ms: None,
    init_time: None,
    rot_separator: None,
};

#[derive(Debug)]
pub struct StrFormatter {
    min_width: usize,
    max_width: usize,
    rot_interval_ms: Option<u64>,
    init_time: Option<Instant>,
    rot_separator: Option<String>,
}

impl StrFormatter {
    pub(super) fn from_args(args: &[Arg]) -> Result<Self> {
        let mut min_width = DEFAULT_STR_MIN_WIDTH;
        let mut max_width = DEFAULT_STR_MAX_WIDTH;
        let mut rot_interval = DEFAULT_STR_ROT_INTERVAL;
        let mut rot_separator = DEFAULT_STR_ROT_SEP;
        for arg in args {
            match arg.key {
                "min_width" | "min_w" => {
                    min_width = arg.val.parse().error("Width must be a positive integer")?;
                }
                "max_width" | "max_w" => {
                    max_width = arg.val.parse().error("Width must be a positive integer")?;
                }
                "width" | "w" => {
                    min_width = arg.val.parse().error("Width must be a positive integer")?;
                    max_width = min_width;
                }
                "rot_interval" => {
                    rot_interval = Some(
                        arg.val
                            .parse()
                            .error("Interval must be a positive number")?,
                    );
                }
                "rot_separator" => {
                    rot_separator = Some(arg.val.to_string());
                }
                other => {
                    return Err(Error::new(format!("Unknown argument for 'str': '{other}'")));
                }
            }
        }
        if max_width < min_width {
            return Err(Error::new(
                "Max width must be greater of equal to min width",
            ));
        }
        if let Some(rot_interval) = rot_interval {
            if rot_interval < 0.1 {
                return Err(Error::new("Interval must be greater than 0.1"));
            }
        }
        Ok(StrFormatter {
            min_width,
            max_width,
            rot_interval_ms: rot_interval.map(|x| (x * 1e3) as u64),
            init_time: Some(Instant::now()),
            rot_separator,
        })
    }
}

impl Formatter for StrFormatter {
    fn format(&self, val: &Value, config: &SharedConfig) -> Result<String, FormatError> {
        match val {
            Value::Text(text) => {
                let text: Vec<&str> = text.graphemes(true).collect();
                let width = text.len();
                Ok(match (self.rot_interval_ms, self.init_time) {
                    (Some(rot_interval_ms), Some(init_time)) if width > self.max_width => {
                        let rot_separator: Vec<&str> = self
                            .rot_separator
                            .as_deref()
                            .unwrap_or("|")
                            .graphemes(true)
                            .collect();
                        let width = width + rot_separator.len(); // Now we include `rot_separator` at the end
                        let step = (init_time.elapsed().as_millis() as u64 / rot_interval_ms)
                            as usize
                            % width;
                        let w1 = self.max_width.min(width - step);
                        text.iter()
                            .chain(rot_separator.iter())
                            .skip(step)
                            .take(w1)
                            .chain(text.iter())
                            .take(self.max_width)
                            .collect_pango_escaped()
                    }
                    _ => text
                        .iter()
                        .chain(repeat(&" ").take(self.min_width.saturating_sub(width)))
                        .take(self.max_width)
                        .collect_pango_escaped(),
                })
            }
            Value::Icon(icon, value) => config.get_icon(icon, *value).map_err(Into::into),
            other => Err(FormatError::IncompatibleFormatter {
                ty: other.type_name(),
                fmt: "str",
            }),
        }
    }

    fn interval(&self) -> Option<Duration> {
        self.rot_interval_ms.map(Duration::from_millis)
    }
}

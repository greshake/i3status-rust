use super::*;

const DEFAULT_BAR_VERTICAL: bool = false;
const DEFAULT_BAR_WIDTH_HORIZONTAL: usize = 5;
const DEFAULT_BAR_WIDTH_VERTICAL: usize = 1;
const DEFAULT_BAR_MAX_VAL: f64 = 100.0;

#[derive(Debug)]
pub struct BarFormatter {
    width: usize,
    max_value: f64,
    vertical: bool,
}

impl BarFormatter {
    pub(super) fn from_args(args: &[Arg]) -> Result<Self> {
        let mut vertical = DEFAULT_BAR_VERTICAL;
        let mut width = None;
        let mut max_value = DEFAULT_BAR_MAX_VAL;
        for arg in args {
            match arg.key {
                "width" | "w" => {
                    width = Some(arg.parse_value()?);
                }
                "max_value" => {
                    max_value = arg.parse_value()?;
                }
                "vertical" | "v" => {
                    vertical = arg.parse_value()?;
                }
                other => {
                    return Err(Error::new(format!("Unknown argument for 'bar': '{other}'")));
                }
            }
        }
        Ok(Self {
            width: width.unwrap_or(match vertical {
                false => DEFAULT_BAR_WIDTH_HORIZONTAL,
                true => DEFAULT_BAR_WIDTH_VERTICAL,
            }),
            max_value,
            vertical,
        })
    }

    #[inline]
    fn norm(&self, val: f64) -> f64 {
        Some(val / self.max_value)
            .filter(|v| v.is_finite())
            .unwrap_or(0.0)
            .clamp(0.0, 1.0)
    }

    #[inline]
    fn format_single_vertical(&self, val: f64) -> char {
        let val = self.norm(val);
        VERTICAL_BAR_CHARS[(val * 8.0) as usize]
    }

    #[inline]
    fn format_horizontal_cell(&self, val: f64, i: usize) -> char {
        let val = self.norm(val);
        let chars_to_fill = val * self.width as f64;
        HORIZONTAL_BAR_CHARS[((chars_to_fill - i as f64).clamp(0.0, 1.0) * 8.0) as usize]
    }

    #[inline]
    fn format_horizontal_bar(&self, val: f64) -> String {
        (0..self.width)
            .map(|i| self.format_horizontal_cell(val, i))
            .collect()
    }
}

const HORIZONTAL_BAR_CHARS: [char; 9] = [' ', '▏', '▎', '▍', '▌', '▋', '▊', '▉', '█'];
const VERTICAL_BAR_CHARS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

impl Formatter for BarFormatter {
    fn format(&self, val: &Value, _config: &SharedConfig) -> Result<String, FormatError> {
        match val {
            Value::Number { val, .. } => {
                if self.vertical {
                    let c = self.format_single_vertical(*val);
                    Ok(std::iter::repeat(c).take(self.width).collect())
                } else {
                    Ok(self.format_horizontal_bar(*val))
                }
            }
            Value::Numbers { vals, .. } => {
                if self.vertical {
                    // NOTE: print at most `width` values as a windowed chart
                    let start = vals.len().saturating_sub(self.width);
                    let shown = vals.len() - start;

                    Ok(std::iter::repeat(0.0)
                        .take(self.width - shown)
                        .chain(vals[start..].iter().copied())
                        .map(|val| self.format_single_vertical(val))
                        .collect())
                } else {
                    // NOTE: print the last value as a horizontal bar
                    let last = vals.last().copied().unwrap_or(0.0);
                    Ok(self.format_horizontal_bar(last))
                }
            }
            other => Err(FormatError::IncompatibleFormatter {
                ty: other.type_name(),
                fmt: "bar",
            }),
        }
    }
}

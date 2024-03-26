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
                    width = Some(arg.val.parse().error("Width must be a positive integer")?);
                }
                "max_value" => {
                    max_value = arg.val.parse().error("Max value must be a number")?;
                }
                "vertical" | "v" => {
                    vertical = arg.val.parse().error("Vertical value must be a bool")?;
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
}

const HORIZONTAL_BAR_CHARS: [char; 9] = [
    ' ', '\u{258f}', '\u{258e}', '\u{258d}', '\u{258c}', '\u{258b}', '\u{258a}', '\u{2589}',
    '\u{2588}',
];

const VERTICAL_BAR_CHARS: [char; 9] = [
    ' ', '\u{2581}', '\u{2582}', '\u{2583}', '\u{2584}', '\u{2585}', '\u{2586}', '\u{2587}',
    '\u{2588}',
];

impl Formatter for BarFormatter {
    fn format(&self, val: &Value, _config: &SharedConfig) -> Result<String, FormatError> {
        match val {
            Value::Number { mut val, .. } => {
                val = (val / self.max_value).clamp(0., 1.);
                if self.vertical {
                    let vert_char = VERTICAL_BAR_CHARS[(val * 8.) as usize];
                    Ok((0..self.width).map(|_| vert_char).collect())
                } else {
                    let chars_to_fill = val * self.width as f64;
                    Ok((0..self.width)
                        .map(|i| {
                            HORIZONTAL_BAR_CHARS
                                [((chars_to_fill - i as f64).clamp(0., 1.) * 8.) as usize]
                        })
                        .collect())
                }
            }
            other => Err(FormatError::IncompatibleFormatter {
                ty: other.type_name(),
                fmt: "bar",
            }),
        }
    }
}

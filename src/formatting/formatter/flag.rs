use super::*;

pub const DEFAULT_FLAG_FORMATTER: FlagFormatter = FlagFormatter;

#[derive(Debug)]
pub struct FlagFormatter;

impl Formatter for FlagFormatter {
    fn infallible_for(&self, kind: ValueKind) -> bool {
        matches!(kind, ValueKind::Flag)
    }

    fn format(&self, val: &Value, _config: &SharedConfig) -> Result<String, FormatError> {
        match val {
            Value::Flag => Ok(String::new()),
            _ => {
                unreachable!()
            }
        }
    }
}

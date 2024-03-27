use super::*;

#[derive(Debug)]
pub struct PangoStrFormatter;

impl PangoStrFormatter {
    pub(super) fn from_args(args: &[Arg]) -> Result<Self> {
        if let Some(arg) = args.first() {
            return Err(Error::new(format!(
                "Unknown argument for 'pango-str': '{}'",
                arg.key
            )));
        }
        Ok(Self)
    }
}

impl Formatter for PangoStrFormatter {
    fn format(&self, val: &Value, config: &SharedConfig) -> Result<String, FormatError> {
        match val {
            Value::Text(x) => Ok(x.clone()), // No escaping
            Value::Icon(icon, value) => config.get_icon(icon, *value).map_err(Into::into),
            other => Err(FormatError::IncompatibleFormatter {
                ty: other.type_name(),
                fmt: "pango-str",
            }),
        }
    }
}

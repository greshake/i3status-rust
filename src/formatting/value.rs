use super::formatter;
use super::unit::Unit;
use super::Metadata;
use chrono::{DateTime, Utc};
use chrono_tz::Tz;

#[derive(Debug, Clone)]
pub struct Value {
    pub inner: ValueInner,
    pub metadata: Metadata,
}

#[derive(Debug, Clone)]
pub enum ValueInner {
    Text(String),
    Icon(String),
    Number { val: f64, unit: Unit },
    Datetime(DateTime<Utc>, Option<Tz>),
    Flag,
}

impl ValueInner {
    pub fn type_name(&self) -> &'static str {
        match self {
            ValueInner::Text(..) => "Text",
            ValueInner::Icon(..) => "Icon",
            ValueInner::Number { .. } => "Number",
            ValueInner::Datetime(..) => "Datetime",
            ValueInner::Flag => "Flag",
        }
    }
}

pub trait IntoF64 {
    fn into_f64(self) -> f64;
}

macro_rules! impl_into_f64 {
    ($($t:ty),+) => {
        $(
            impl IntoF64 for $t {
                fn into_f64(self) -> f64 {
                    self as _
                }
            }
        )+
    }
}
impl_into_f64!(f64, f32, i64, u64, i32, u32, i16, u16, i8, u8, usize, isize);

/// Constructors
impl Value {
    pub fn new(val: ValueInner) -> Self {
        Self {
            inner: val,
            metadata: Default::default(),
        }
    }

    pub fn flag() -> Self {
        Self::new(ValueInner::Flag)
    }

    pub fn datetime(datetime: DateTime<Utc>, tz: Option<Tz>) -> Self {
        Self::new(ValueInner::Datetime(datetime, tz))
    }

    pub fn icon(icon: String) -> Self {
        Self::new(ValueInner::Icon(icon))
    }

    pub fn text(text: String) -> Self {
        Self::new(ValueInner::Text(text))
    }

    pub fn number_unit(val: impl IntoF64, unit: Unit) -> Self {
        Self::new(ValueInner::Number {
            val: val.into_f64(),
            unit,
        })
    }

    pub fn bytes(val: impl IntoF64) -> Self {
        Self::number_unit(val, Unit::Bytes)
    }
    pub fn bits(val: impl IntoF64) -> Self {
        Self::number_unit(val, Unit::Bits)
    }
    pub fn percents(val: impl IntoF64) -> Self {
        Self::number_unit(val, Unit::Percents)
    }
    pub fn degrees(val: impl IntoF64) -> Self {
        Self::number_unit(val, Unit::Degrees)
    }
    pub fn seconds(val: impl IntoF64) -> Self {
        Self::number_unit(val, Unit::Seconds)
    }
    pub fn watts(val: impl IntoF64) -> Self {
        Self::number_unit(val, Unit::Watts)
    }
    pub fn hertz(val: impl IntoF64) -> Self {
        Self::number_unit(val, Unit::Hertz)
    }
    pub fn number(val: impl IntoF64) -> Self {
        Self::number_unit(val, Unit::None)
    }
}

/// Set options
impl Value {
    pub fn with_instance(mut self, instance: &'static str) -> Self {
        self.metadata.instance = Some(instance);
        self
    }

    pub fn underline(mut self, val: bool) -> Self {
        self.metadata.underline = val;
        self
    }

    pub fn italic(mut self, val: bool) -> Self {
        self.metadata.italic = val;
        self
    }

    pub fn default_formatter(&self) -> &'static dyn formatter::Formatter {
        match &self.inner {
            ValueInner::Text(_) | ValueInner::Icon(_) => &formatter::DEFAULT_STRING_FORMATTER,
            ValueInner::Number { .. } => &formatter::DEFAULT_NUMBER_FORMATTER,
            ValueInner::Datetime { .. } => &*formatter::DEFAULT_DATETIME_FORMATTER,
            ValueInner::Flag => &formatter::DEFAULT_FLAG_FORMATTER,
        }
    }
}

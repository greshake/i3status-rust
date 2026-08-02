use std::borrow::Cow;
use std::time::Duration;

use super::Metadata;
use super::formatter;
use super::unit::Unit;
use chrono::{DateTime, Utc};
use chrono_tz::Tz;

#[derive(Debug, Clone)]
pub struct Value {
    pub inner: ValueInner,
    pub metadata: Metadata,
}

/// A checked icon name. Constructible only inside this module (via the
/// token-gated [`Value`] constructors), which is what makes
/// [`ValueInner::Icon`] impossible to build outside the plan-handle path —
/// including via `Value::new` or by mutating `value.inner`.
#[derive(Debug, Clone)]
pub struct IconName(Cow<'static, str>);

impl std::ops::Deref for IconName {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for IconName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone)]
pub enum ValueInner {
    Text(String),
    Icon(IconName, Option<f64>),
    Number { val: f64, unit: Unit },
    Datetime(DateTime<Utc>, Option<Tz>),
    Duration(Duration),
    Flag,
}

impl ValueInner {
    pub fn type_name(&self) -> &'static str {
        match self {
            ValueInner::Text(..) => "Text",
            ValueInner::Icon(..) => "Icon",
            ValueInner::Number { .. } => "Number",
            ValueInner::Datetime(..) => "Datetime",
            ValueInner::Duration(..) => "Duration",
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

    pub fn duration(duration: Duration) -> Self {
        Self::new(ValueInner::Duration(duration))
    }

    /// An icon value. The token proves the name was checked against the
    /// block's prepared contract: it can only be minted by
    /// [`crate::block_plan::OutputHandle`], so blocks cannot construct icon
    /// values that bypass their plan.
    pub fn icon<S>(name: S, token: crate::block_plan::IconToken) -> Self
    where
        S: Into<Cow<'static, str>>,
    {
        let _ = token;
        Self::new(ValueInner::Icon(IconName(name.into()), None))
    }

    pub fn icon_progression<S>(name: S, value: f64, token: crate::block_plan::IconToken) -> Self
    where
        S: Into<Cow<'static, str>>,
    {
        let _ = token;
        Self::new(ValueInner::Icon(IconName(name.into()), Some(value)))
    }

    /// Test-only escape hatch for constructing icon values without a plan.
    #[cfg(test)]
    pub(crate) fn test_icon<S>(name: S) -> Self
    where
        S: Into<Cow<'static, str>>,
    {
        Self::new(ValueInner::Icon(IconName(name.into()), None))
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
    pub fn degrees_c(val: impl IntoF64) -> Self {
        Self::number_unit(val, Unit::DegreesC)
    }
    pub fn degrees_f(val: impl IntoF64) -> Self {
        Self::number_unit(val, Unit::DegreesF)
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
            ValueInner::Text(_) | ValueInner::Icon(..) => &formatter::DEFAULT_STRING_FORMATTER,
            ValueInner::Number { .. } => &formatter::DEFAULT_NUMBER_FORMATTER,
            ValueInner::Datetime { .. } => &*formatter::DEFAULT_DATETIME_FORMATTER,
            ValueInner::Duration { .. } => &formatter::DEFAULT_DURATION_FORMATTER,
            ValueInner::Flag => &formatter::DEFAULT_FLAG_FORMATTER,
        }
    }
}

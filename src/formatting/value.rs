use super::unit::Unit;
use smartstring::alias::String;

#[derive(Debug, Clone)]
pub enum Value {
    Text(String),
    Icon(String),
    Number { val: f64, unit: Unit, icon: String },
    Flag,
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

/// Constuctors
impl Value {
    pub fn text(text: String) -> Self {
        Self::Text(text)
    }

    pub fn number_unit(val: impl IntoF64, unit: Unit) -> Self {
        Self::Number {
            val: val.into_f64(),
            unit,
            icon: String::new(),
        }
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
    pub fn icon(self, icon: String) -> Self {
        match self {
            Self::Number { val, unit, .. } => Self::Number { val, unit, icon },
            _ => todo!(),
        }
    }
}

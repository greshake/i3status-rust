use crate::errors::*;
use serde::de::{self, Deserializer, Visitor};
use serde::Deserialize;
use smart_default::SmartDefault;
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq, SmartDefault)]
pub enum Separator {
    #[default]
    Native,
    Custom(String),
}

impl FromStr for Separator {
    type Err = Error;

    fn from_str(separator: &str) -> Result<Self, Self::Err> {
        Ok(if separator == "native" {
            Self::Native
        } else {
            Self::Custom(separator.into())
        })
    }
}

impl<'de> Deserialize<'de> for Separator {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct SeparatorVisitor;

        impl<'de> Visitor<'de> for SeparatorVisitor {
            type Value = Separator;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a separator string or 'native'")
            }

            fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                s.parse().serde_error()
            }
        }

        deserializer.deserialize_any(SeparatorVisitor)
    }
}

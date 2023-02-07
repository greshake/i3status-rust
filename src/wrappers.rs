use crate::errors::{Result, ResultExt};
use serde::de::{self, Deserialize, Deserializer};
use std::borrow::Cow;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Seconds<const ALLOW_ONCE: bool = true>(pub Duration);

impl<const ALLOW_ONCE: bool> From<u64> for Seconds<ALLOW_ONCE> {
    fn from(v: u64) -> Self {
        Self::new(v)
    }
}

impl<const ALLOW_ONCE: bool> Seconds<ALLOW_ONCE> {
    pub fn new(value: u64) -> Self {
        Self(Duration::from_secs(value))
    }

    pub fn timer(self) -> tokio::time::Interval {
        let mut timer = tokio::time::interval_at(tokio::time::Instant::now() + self.0, self.0);
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        timer
    }

    pub fn seconds(self) -> u64 {
        self.0.as_secs()
    }
}

impl<'de, const ALLOW_ONCE: bool> Deserialize<'de> for Seconds<ALLOW_ONCE> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct SecondsVisitor<const ALLOW_ONCE: bool>;

        impl<'de, const ALLOW_ONCE: bool> de::Visitor<'de> for SecondsVisitor<ALLOW_ONCE> {
            type Value = Seconds<ALLOW_ONCE>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("\"once\", i64 or f64")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                if ALLOW_ONCE && v == "once" {
                    Ok(Seconds(Duration::from_secs(60 * 60 * 24 * 365)))
                } else {
                    Err(E::custom(format!("'{v}' is not a valid duration")))
                }
            }

            fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(Seconds(Duration::from_secs(v as u64)))
            }

            fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(Seconds(Duration::from_secs_f64(v)))
            }
        }

        deserializer.deserialize_any(SecondsVisitor)
    }
}

#[derive(Debug, Clone)]
pub struct ShellString(pub Cow<'static, str>);

impl<T> From<T> for ShellString
where
    T: Into<Cow<'static, str>>,
{
    fn from(v: T) -> Self {
        Self(v.into())
    }
}

impl<'de> Deserialize<'de> for ShellString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct Visitor;

        impl<'de> de::Visitor<'de> for Visitor {
            type Value = ShellString;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("text")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(ShellString(v.to_string().into()))
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}

impl ShellString {
    pub fn new<T: Into<Cow<'static, str>>>(value: T) -> Self {
        Self(value.into())
    }

    pub fn expand(&self) -> Result<Cow<str>> {
        shellexpand::full(&self.0).error("Failed to expand string")
    }
}

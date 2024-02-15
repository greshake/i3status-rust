use crate::errors::*;

use serde::de::{self, Deserialize, Deserializer};
use std::borrow::Cow;
use std::fmt::{self, Display};
use std::marker::PhantomData;
use std::ops::RangeInclusive;
use std::str::FromStr;
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

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
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

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
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

/// A map with keys being ranges.
#[derive(Debug, Default, Clone)]
pub struct RangeMap<K, V>(Vec<(RangeInclusive<K>, V)>);

impl<K, V> RangeMap<K, V> {
    pub fn get(&self, key: &K) -> Option<&V>
    where
        K: PartialOrd,
    {
        self.0
            .iter()
            .find_map(|(k, v)| k.contains(key).then_some(v))
    }
}

impl<K, V> From<Vec<(RangeInclusive<K>, V)>> for RangeMap<K, V> {
    fn from(vec: Vec<(RangeInclusive<K>, V)>) -> Self {
        Self(vec)
    }
}

impl<'de, K, V> Deserialize<'de> for RangeMap<K, V>
where
    K: FromStr,
    K::Err: Display,
    V: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct Visitor<K, V>(PhantomData<(K, V)>);

        impl<'de, K, V> de::Visitor<'de> for Visitor<K, V>
        where
            K: FromStr,
            K::Err: Display,
            V: Deserialize<'de>,
        {
            type Value = RangeMap<K, V>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("range map")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: de::MapAccess<'de>,
            {
                let mut vec = Vec::with_capacity(map.size_hint().unwrap_or(2));
                while let Some((range, val)) = map.next_entry::<String, V>()? {
                    let (start, end) = range
                        .split_once("..")
                        .error("invalid range")
                        .serde_error()?;
                    let start: K = start.parse().serde_error()?;
                    let end: K = end.parse().serde_error()?;
                    vec.push((start..=end, val));
                }
                Ok(RangeMap(vec))
            }
        }

        deserializer.deserialize_map(Visitor(PhantomData))
    }
}

#[derive(Debug, Clone)]
pub struct SerdeRegex(pub regex::Regex);

impl PartialEq for SerdeRegex {
    fn eq(&self, other: &Self) -> bool {
        self.0.as_str() == other.0.as_str()
    }
}

impl Eq for SerdeRegex {}

impl std::hash::Hash for SerdeRegex {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.as_str().hash(state);
    }
}

impl<'de> Deserialize<'de> for SerdeRegex {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct Visitor;

        impl<'de> de::Visitor<'de> for Visitor {
            type Value = SerdeRegex;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a regex")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                regex::Regex::new(v).map(SerdeRegex).map_err(E::custom)
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}

/// Display a slice. Similar to Debug impl for slice, but uses Display impl for elements.
pub struct DisplaySlice<'a, T>(pub &'a [T]);

impl<'a, T: Display> Display for DisplaySlice<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        struct DisplayAsDebug<'a, T>(&'a T);
        impl<'a, T: Display> fmt::Debug for DisplayAsDebug<'a, T> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(self.0, f)
            }
        }
        f.debug_list()
            .entries(self.0.iter().map(DisplayAsDebug))
            .finish()
    }
}

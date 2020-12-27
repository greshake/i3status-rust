use std::collections::{BTreeMap, HashMap as Map};
use std::fmt;
use std::marker::PhantomData;
use std::ops::Deref;
use std::str::FromStr;
use std::time::Duration;

use crate::blocks::Update;
use chrono::{DateTime, Local};
use serde::de::{self, Deserialize, DeserializeSeed, Deserializer};
use toml::{self, value};

pub fn deserialize_update<'de, D>(deserializer: D) -> Result<Update, D::Error>
where
    D: Deserializer<'de>,
{
    struct UpdateWrapper;

    impl<'de> de::Visitor<'de> for UpdateWrapper {
        type Value = Update;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str(r#"i64, f64 or "once" "#)
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(Duration::from_secs(value as u64).into())
        }

        fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(Duration::new(0, (value * 1_000_000_000f64) as u32).into())
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            if value == "once" {
                Ok(Update::Once)
            } else {
                Err(de::Error::custom(r#"expected "once""#))
            }
        }
    }

    deserializer.deserialize_any(UpdateWrapper)
}

pub fn deserialize_duration<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: Deserializer<'de>,
{
    struct DurationWrapper;

    impl<'de> de::Visitor<'de> for DurationWrapper {
        type Value = Duration;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("i64, f64 or map")
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(Duration::from_secs(value as u64))
        }

        fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(Duration::new(0, (value * 1_000_000_000f64) as u32))
        }

        fn visit_map<A>(self, visitor: A) -> Result<Self::Value, A::Error>
        where
            A: de::MapAccess<'de>,
        {
            Deserialize::deserialize(de::value::MapAccessDeserializer::new(visitor))
        }
    }

    deserializer.deserialize_any(DurationWrapper)
}

pub fn deserialize_opt_duration<'de, D>(deserializer: D) -> Result<Option<Duration>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_duration(deserializer).map(Some)
}

pub struct MapType<T, V>(pub PhantomData<T>, pub PhantomData<V>);

macro_rules! map_type {
    ( $name:ident, $value:ty; $fromstr_ident:ident => $fromstr_expr:expr ) => {
        #[derive(Deserialize, Debug, Default)]
        struct $name(::std::collections::HashMap<String, $value>);

        impl Deref for $name {
            type Target = ::std::collections::HashMap<String, $value>;

            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl From<::std::collections::HashMap<String, $value>> for $name {
            fn from(m: ::std::collections::HashMap<String, $value>) -> Self {
                $name(m)
            }
        }

        impl FromStr for $name {
            type Err = String;

            fn from_str($fromstr_ident: &str) -> Result<Self, Self::Err> {
                $fromstr_expr
            }
        }
    };
}

impl<'de, T, V> DeserializeSeed<'de> for MapType<T, V>
where
    T: Deserialize<'de>
        + Default
        + FromStr<Err = String>
        + From<Map<String, V>>
        + Deref<Target = Map<String, V>>,
    V: Deserialize<'de> + Clone,
{
    type Value = Map<String, V>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(self)
    }
}

impl<'de, T, V> de::Visitor<'de> for MapType<T, V>
where
    T: Deserialize<'de>
        + Default
        + FromStr<Err = String>
        + From<Map<String, V>>
        + Deref<Target = Map<String, V>>,
    V: Deserialize<'de> + Clone,
{
    type Value = Map<String, V>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("string, seq or map")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        let t: T = FromStr::from_str(value).map_err(de::Error::custom)?;
        Ok(t.clone())
    }

    fn visit_seq<A>(self, mut visitor: A) -> Result<Self::Value, A::Error>
    where
        A: de::SeqAccess<'de>,
    {
        let mut vec: Vec<Self::Value> = Vec::new();
        while let Some(element) =
            visitor.next_element_seed(MapType::<T, V>(PhantomData, PhantomData))?
        {
            vec.push(element);
        }

        if vec.is_empty() {
            Err(de::Error::custom("seq is empty"))
        } else {
            let mut combined = vec.remove(0);
            for other in vec {
                combined.extend(other);
            }
            Ok(combined)
        }
    }

    /// If the TOML fragment is a map (table), it has to look something like this:
    ///
    /// ```toml
    /// [mytype]
    /// name = "predefined-type"
    /// [mytype.overrides]
    /// field1 = "override field 1"
    /// field2 = "override field 2"
    /// ```
    ///
    /// The `name` field will be recursively deserialized using `visit_str` or `visit_seq`. The
    /// overrides field will be deserialized into a `Map<String, V>` and then combined with what
    /// the deserialization of `name` delivered.
    fn visit_map<A>(self, visitor: A) -> Result<Self::Value, A::Error>
    where
        A: de::MapAccess<'de>,
    {
        let mut map: BTreeMap<String, value::Value> =
            Deserialize::deserialize(de::value::MapAccessDeserializer::new(visitor))?;
        let mut combined: Map<String, V> = Map::new();

        if let Some(raw_names) = map.remove("name") {
            combined.extend(
                raw_names
                    .deserialize_any(MapType::<T, V>(PhantomData, PhantomData))
                    .map_err(|e: toml::de::Error| de::Error::custom(e.to_string()))?,
            );
        }
        if let Some(raw_overrides) = map.remove("overrides") {
            let overrides: Map<String, V> = Map::<String, V>::deserialize(raw_overrides)
                .map_err(|e: toml::de::Error| de::Error::custom(e.to_string()))?;
            combined.extend(overrides);
        }

        if combined.is_empty() {
            Err(de::Error::custom(
                "missing all fields (`name`, `overrides`)",
            ))
        } else {
            Ok(combined)
        }
    }
}

pub fn deserialize_local_timestamp<'de, D>(deserializer: D) -> Result<DateTime<Local>, D::Error>
where
    D: Deserializer<'de>,
{
    use chrono::TimeZone;
    i64::deserialize(deserializer).map(|seconds| Local.timestamp(seconds, 0))
}

#[cfg(test)]
mod tests {
    use crate::blocks::Update;
    use crate::blocks::Update::{Every, Once};
    use crate::de::{deserialize_duration, deserialize_update};
    use serde_derive::Deserialize;
    use std::time::Duration;

    #[derive(Deserialize, Debug, Clone)]
    #[serde(deny_unknown_fields)]
    pub struct DurationConfig {
        /// Update interval in seconds
        #[serde(deserialize_with = "deserialize_duration")]
        pub interval: Duration,
    }

    #[test]
    fn test_deserialize_duration() {
        let duration_toml = r#""interval"= 5"#;
        let deserialized: DurationConfig = toml::from_str(duration_toml).unwrap();
        assert_eq!(Duration::new(5, 0), deserialized.interval);
        let duration_toml = r#""interval"= 0.5"#;
        let deserialized: DurationConfig = toml::from_str(duration_toml).unwrap();
        assert_eq!(Duration::new(0, 500_000_000), deserialized.interval);
    }

    #[derive(Deserialize, Debug, Clone)]
    #[serde(deny_unknown_fields)]
    pub struct UpdateConfig {
        /// Update interval in seconds
        #[serde(deserialize_with = "deserialize_update")]
        pub interval: Update,
    }

    #[test]
    fn test_deserialize_update() {
        let duration_toml = r#""interval"= 5"#;
        let deserialized: UpdateConfig = toml::from_str(duration_toml).unwrap();
        assert_eq!(Every(Duration::new(5, 0)), deserialized.interval);
        let duration_toml = r#""interval"= 0.5"#;
        let deserialized: UpdateConfig = toml::from_str(duration_toml).unwrap();
        assert_eq!(Every(Duration::new(0, 500_000_000)), deserialized.interval);
        let duration_toml = r#""interval"= "once""#;
        let deserialized: UpdateConfig = toml::from_str(duration_toml).unwrap();
        assert_eq!(Once, deserialized.interval);
    }
}

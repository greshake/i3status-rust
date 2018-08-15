use serde::de::{self, Deserialize, DeserializeSeed, Deserializer};
use std::collections::{BTreeMap, HashMap as Map};
use std::error::Error;
use std::fmt;
use std::marker::PhantomData;
use std::ops::Deref;
use std::str::FromStr;
use std::time::Duration;
use toml::{self, value};
use chrono_tz::Tz;

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
    }
}

impl<'de, T, V> DeserializeSeed<'de> for MapType<T, V>
where
    T: Deserialize<'de> + Default + FromStr<Err = String> + From<Map<String, V>> + Deref<Target = Map<String, V>>,
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
    T: Deserialize<'de> + Default + FromStr<Err = String> + From<Map<String, V>> + Deref<Target = Map<String, V>>,
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
        while let Some(element) = visitor
            .next_element_seed(MapType::<T, V>(PhantomData, PhantomData))?
        {
            vec.push(element);
        }

        if vec.is_empty() {
            Err(de::Error::custom("seq is empty"))
        } else {
            let mut combined = vec.remove(0).clone();
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
        let mut map: BTreeMap<String, value::Value> = Deserialize::deserialize(de::value::MapAccessDeserializer::new(visitor))?;
        let mut combined: Map<String, V> = Map::new();

        if let Some(raw_names) = map.remove("name") {
            combined.extend(
                raw_names
                    .deserialize_any(MapType::<T, V>(PhantomData, PhantomData))
                    .map_err(|e: toml::de::Error| de::Error::custom(e.description()))?,
            );
        }
        if let Some(raw_overrides) = map.remove("overrides") {
            let overrides: Map<String, V> = Map::<String, V>::deserialize(raw_overrides)
                .map_err(|e: toml::de::Error| de::Error::custom(e.description()))?;
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

pub fn deserialize_timezone<'de, D>(deserializer: D) -> Result<Option<Tz>, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Tz::from_str(&s).map(|tz| Some(tz)).map_err(de::Error::custom)
}

use std::fmt;
use std::time::Duration;

use crate::blocks::Update;
use chrono::{DateTime, Local};
use serde::de::{self, Deserialize, Deserializer};

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

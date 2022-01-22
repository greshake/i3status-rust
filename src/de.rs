use chrono::{DateTime, Local};
use serde::de::{Deserialize, Deserializer};

pub fn deserialize_local_timestamp<'de, D>(deserializer: D) -> Result<DateTime<Local>, D::Error>
where
    D: Deserializer<'de>,
{
    use chrono::TimeZone;
    i64::deserialize(deserializer).map(|seconds| Local.timestamp(seconds, 0))
}

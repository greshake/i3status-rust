use de::*;
use icons;
use serde::de::{self, Deserialize, Deserializer};
use serde_json::Value;
use std::collections::HashMap as Map;
use std::fmt;
use std::marker::PhantomData;
use std::ops::Deref;
use std::str::FromStr;
use themes::{self, Theme};

#[derive(Deserialize, Debug, Default, Clone)]
pub struct Config {
    pub blocks: Map<String, Value>,
    #[serde(default = "icons::default", deserialize_with = "deserialize_icons")]
    pub icons: Map<String, String>,
    #[serde(default = "themes::default", deserialize_with = "deserialize_string_or_struct")]
    pub theme: Theme,
}


fn deserialize_icons<'de, D>(deserializer: D) -> Result<Map<String, String>, D::Error>
where
    D: Deserializer<'de>
{
    map_type!(Icons, String, String;
              s => Ok(Icons(icons::get_icons(s).ok_or_else(|| "cannot find specified icons")?)));

    deserializer.deserialize_any(MapType::<Icons, String, String>(PhantomData, PhantomData, PhantomData))
}

fn deserialize_string_or_struct<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    T: Deserialize<'de> + FromStr<Err = String>,
    D: Deserializer<'de>
{
    struct StringOrStruct<T>(PhantomData<T>);

    impl<'de, T> de::Visitor<'de> for StringOrStruct<T>
    where
        T: Deserialize<'de> + FromStr<Err = String>,
    {
        type Value = T;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("string or map")
        }

        fn visit_str<E>(self, value: &str) -> Result<T, E>
        where
            E: de::Error
        {
            FromStr::from_str(value).map_err(de::Error::custom)
        }

        fn visit_map<M>(self, visitor: M) -> Result<T, M::Error>
        where
            M: de::MapAccess<'de>
        {
            Deserialize::deserialize(de::value::MapAccessDeserializer::new(visitor))
        }
    }

    deserializer.deserialize_any(StringOrStruct(PhantomData))
}

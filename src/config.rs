use icons;
use serde::de::{self, Deserialize, Deserializer};
use serde_json::Value;
use std::collections::HashMap as Map;
use std::fmt;
use std::marker::PhantomData;
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
    struct Icons;

    impl<'de> de::Visitor<'de> for Icons {
        type Value = Map<String, String>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("string or map")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error
        {
            icons::get_icons(value)
                .ok_or_else(|| de::Error::custom("couldn't deserialize icons"))
        }

        fn visit_map<M>(self, visitor: M) -> Result<Self::Value, M::Error>
        where
            M: de::MapAccess<'de>
        {
            Deserialize::deserialize(de::value::MapAccessDeserializer::new(visitor))
        }
    }

    deserializer.deserialize_any(Icons)
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

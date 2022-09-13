use super::{template::FormatTemplate, Format, FormatInner};
use crate::errors::*;
use serde::de::{MapAccess, Visitor};
use serde::{de, Deserialize, Deserializer};
use std::fmt;
use std::sync::Arc;

#[derive(Debug, Default)]
pub struct Config {
    full: Option<FormatTemplate>,
    short: Option<FormatTemplate>,
}

#[derive(Debug, Default)]
pub struct DummyConfig {
    pub full: Option<String>,
    pub short: Option<String>,
}

impl Config {
    pub fn with_default(self, default_full: &str) -> Result<Format> {
        let full = match self.full {
            Some(full) => full,
            None => default_full.parse()?,
        };

        let mut intervals = Vec::new();
        full.init_intervals(&mut intervals);
        if let Some(short) = &self.short {
            short.init_intervals(&mut intervals);
        }

        Ok(Arc::new(FormatInner {
            full,
            short: self.short,
            intervals,
        }))
    }
}

impl<'de> Deserialize<'de> for Config {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "lowercase")]
        enum Field {
            Full,
            Short,
        }

        struct FormatTemplateVisitor;

        impl<'de> Visitor<'de> for FormatTemplateVisitor {
            type Value = Config;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("format structure")
            }

            /// Handle configs like:
            ///
            /// ```toml
            /// format = "{layout}"
            /// ```
            fn visit_str<E>(self, full: &str) -> Result<Config, E>
            where
                E: de::Error,
            {
                Ok(Config {
                    full: Some(full.parse().serde_error()?),
                    short: None,
                })
            }

            /// Handle configs like:
            ///
            /// ```toml
            /// [block.format]
            /// full = "{layout}"
            /// short = "{layout^2}"
            /// ```
            fn visit_map<V>(self, mut map: V) -> Result<Config, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut full: Option<FormatTemplate> = None;
                let mut short: Option<FormatTemplate> = None;
                while let Some(key) = map.next_key()? {
                    match key {
                        Field::Full => {
                            if full.is_some() {
                                return Err(de::Error::duplicate_field("full"));
                            }
                            full = Some(map.next_value::<String>()?.parse().serde_error()?);
                        }
                        Field::Short => {
                            if short.is_some() {
                                return Err(de::Error::duplicate_field("short"));
                            }
                            short = Some(map.next_value::<String>()?.parse().serde_error()?);
                        }
                    }
                }
                Ok(Config { full, short })
            }
        }

        deserializer.deserialize_any(FormatTemplateVisitor)
    }
}

impl<'de> Deserialize<'de> for DummyConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "lowercase")]
        enum Field {
            Full,
            Short,
        }

        struct FormatTemplateVisitor;

        impl<'de> Visitor<'de> for FormatTemplateVisitor {
            type Value = DummyConfig;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("format structure")
            }

            fn visit_str<E>(self, full: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(DummyConfig {
                    full: Some(full.into()),
                    short: None,
                })
            }

            fn visit_map<V>(self, mut map: V) -> Result<Self::Value, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut full: Option<String> = None;
                let mut short: Option<String> = None;
                while let Some(key) = map.next_key()? {
                    match key {
                        Field::Full => {
                            if full.is_some() {
                                return Err(de::Error::duplicate_field("full"));
                            }
                            full = Some(map.next_value::<String>()?);
                        }
                        Field::Short => {
                            if short.is_some() {
                                return Err(de::Error::duplicate_field("short"));
                            }
                            short = Some(map.next_value::<String>()?);
                        }
                    }
                }
                Ok(DummyConfig { full, short })
            }
        }

        deserializer.deserialize_any(FormatTemplateVisitor)
    }
}

use super::{template::FormatTemplate, Format};
use crate::errors::*;
use serde::de::{MapAccess, Visitor};
use serde::{de, Deserialize, Deserializer};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Default, Clone)]
pub struct Config {
    pub full: Option<FormatTemplate>,
    pub short: Option<FormatTemplate>,
}

impl Config {
    pub fn with_default(&self, default_full: &str) -> Result<Format> {
        self.with_defaults(default_full, "")
    }

    pub fn with_defaults(&self, default_full: &str, default_short: &str) -> Result<Format> {
        let full = match self.full.clone() {
            Some(full) => full,
            None => default_full.parse()?,
        };

        let short = match self.short.clone() {
            Some(short) => short,
            None => default_short.parse()?,
        };

        let mut intervals = Vec::new();
        full.init_intervals(&mut intervals);
        short.init_intervals(&mut intervals);

        Ok(Format {
            full,
            short,
            intervals,
        })
    }

    pub fn with_default_config(&self, default_config: &Self) -> Format {
        let full = self
            .full
            .clone()
            .or_else(|| default_config.full.clone())
            .unwrap_or_default();
        let short = self
            .short
            .clone()
            .or_else(|| default_config.short.clone())
            .unwrap_or_default();

        let mut intervals = Vec::new();
        full.init_intervals(&mut intervals);
        short.init_intervals(&mut intervals);

        Format {
            full,
            short,
            intervals,
        }
    }

    pub fn with_default_format(&self, default_format: &Format) -> Format {
        let full = self
            .full
            .clone()
            .unwrap_or_else(|| default_format.full.clone());
        let short = self
            .short
            .clone()
            .unwrap_or_else(|| default_format.short.clone());

        let mut intervals = Vec::new();
        full.init_intervals(&mut intervals);
        short.init_intervals(&mut intervals);

        Format {
            full,
            short,
            intervals,
        }
    }
}

impl FromStr for Config {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self {
            full: Some(s.parse()?),
            short: None,
        })
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
                full.parse().serde_error()
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

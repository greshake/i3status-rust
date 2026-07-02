use super::{Format, MultiFormat, template::FormatTemplate};
use crate::errors::*;
use itertools::Itertools as _;
use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer, de};
use smart_default::SmartDefault;
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

        Ok(Format::new(full, short))
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

        Format::new(full, short)
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

        Format::new(full, short)
    }
}

impl From<Config> for Format {
    fn from(config: Config) -> Self {
        let full = config.full.unwrap_or_default();
        let short = config.short.unwrap_or_default();

        Format::new(full, short)
    }
}

#[derive(Debug, Clone, SmartDefault)]
pub enum MaybeMultiConfig {
    #[default]
    Split {
        config: Option<Config>,
        config_alt: Option<Config>,
    },
    Multiple {
        configs: Vec<Config>,
    },
}

impl MaybeMultiConfig {
    pub fn with_default(&self, default_full: &str) -> Result<MultiFormat> {
        self.with_defaults(default_full, "")
    }

    pub fn with_defaults(&self, default_full: &str, default_short: &str) -> Result<MultiFormat> {
        Ok(MultiFormat::new(match self.clone() {
            MaybeMultiConfig::Multiple { configs } => configs
                .into_iter()
                .enumerate()
                .map(|(i, config)| {
                    if i == 0 {
                        config.with_defaults(default_full, default_short)
                    } else {
                        Ok(config.into())
                    }
                })
                .collect::<Result<Vec<_>>>()?,
            MaybeMultiConfig::Split { config, config_alt } => {
                let mut formats = vec![
                    config
                        .unwrap_or_default()
                        .with_defaults(default_full, default_short)?,
                ];

                if let Some(config_alt) = config_alt {
                    formats.push(config_alt.into());
                }
                formats
            }
        }))
    }

    pub fn with_default_formats(&self, default_formats: &[Format]) -> MultiFormat {
        MultiFormat::new(
            match self.clone() {
                MaybeMultiConfig::Multiple { configs } => configs,
                MaybeMultiConfig::Split { config, config_alt } => {
                    vec![config.unwrap_or_default(), config_alt.unwrap_or_default()]
                }
            }
            .into_iter()
            .zip_longest(default_formats)
            .filter_map(|pair| match pair {
                itertools::EitherOrBoth::Both(config, default_format) => {
                    Some(config.with_default_format(default_format))
                }
                itertools::EitherOrBoth::Left(config) => Some(config.into()),
                itertools::EitherOrBoth::Right(_) => None,
            })
            .collect(),
        )
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

impl<'de> Deserialize<'de> for MaybeMultiConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum MaybeVecConfig {
            Multiple(Vec<Config>),
            Single(Config),
        }

        struct MaybeMultiConfigVisitor;

        impl<'de> Visitor<'de> for MaybeMultiConfigVisitor {
            type Value = MaybeMultiConfig;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("multiformat structure")
            }

            /// Handle configs like:
            ///
            /// ```toml
            /// format = "{layout}"
            /// ```
            ///
            /// ```toml
            /// format = ["{layout}"]
            /// ```
            ///
            /// ```toml
            /// [block.format]
            /// full = "{layout}"
            /// short = "{layout^2}"
            /// ```
            ///
            /// ```toml
            /// [[block.format]]
            /// full = "{layout}"
            /// short = "{layout^2}"
            /// ```
            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut format: Option<MaybeVecConfig> = None;
                let mut format_alt: Option<Config> = None;

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "format" => {
                            if format.is_some() {
                                return Err(de::Error::duplicate_field("format"));
                            }

                            // This ensures Config parsing errors are surfaced
                            let maybe_vec_config = match map.next_value()? {
                                serde_json::Value::Array(arr) => MaybeVecConfig::Multiple(
                                    serde_json::from_value(serde_json::Value::Array(arr))
                                        .map_err(|e| de::Error::custom(e.to_string()))?,
                                ),
                                value => {
                                    MaybeVecConfig::Single(serde_json::from_value(value).map_err(
                                        |e: serde_json::Error| de::Error::custom(e.to_string()),
                                    )?)
                                }
                            };
                            format = Some(maybe_vec_config);
                        }
                        "format_alt" => {
                            if format_alt.is_some() {
                                return Err(de::Error::duplicate_field("format_alt"));
                            }
                            format_alt = Some(map.next_value()?);
                        }
                        unknown => {
                            return Err(de::Error::unknown_field(
                                unknown,
                                &["format", "format_alt"],
                            ));
                        }
                    }
                }

                match format {
                    Some(MaybeVecConfig::Multiple(configs)) => {
                        if format_alt.is_some() {
                            return Err(de::Error::custom(
                                "data did not match any variant of untagged enum MaybeMultiConfig",
                            ));
                        }
                        if configs.is_empty() {
                            return Err(de::Error::custom(
                                "An empty list of configs is not allowed",
                            ));
                        }
                        Ok(MaybeMultiConfig::Multiple { configs })
                    }
                    Some(MaybeVecConfig::Single(config)) => Ok(MaybeMultiConfig::Split {
                        config: Some(config),
                        config_alt: format_alt,
                    }),
                    None => Ok(MaybeMultiConfig::Split {
                        config: None,
                        config_alt: format_alt,
                    }),
                }
            }
        }

        deserializer.deserialize_map(MaybeMultiConfigVisitor)
    }
}

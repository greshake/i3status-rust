use crate::de::*;
use crate::icons;
use toml::value;
use serde::de::{Deserialize, Deserializer, Error};
use std::collections::HashMap as Map;
use std::marker::PhantomData;
use std::ops::Deref;
use std::str::FromStr;
use crate::themes::{Theme, ThemeConfig};

#[derive(Deserialize, Debug, Clone)]
pub struct Config {
    #[serde(default = "icons::default", deserialize_with = "deserialize_icons")]
    pub icons: Map<String, String>,
    #[serde(deserialize_with = "deserialize_themes")]
    pub theme: Theme,
    #[serde(rename = "block", deserialize_with = "deserialize_blocks")]
    pub blocks: Vec<(String, value::Value)>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            icons: icons::default(),
            theme: Theme::default(),
            blocks: Vec::new(),
        }
    }
}

fn deserialize_blocks<'de, D>(deserializer: D) -> Result<Vec<(String, value::Value)>, D::Error>
where
    D: Deserializer<'de>,
{
    let mut blocks: Vec<(String, value::Value)> = Vec::new();
    let raw_blocks: Vec<value::Table> = Deserialize::deserialize(deserializer)?;
    for mut entry in raw_blocks {
        if let Some(name) = entry.remove("block") {
            if let Some(name) = name.as_str() {
                blocks.push((name.to_owned(), value::Value::Table(entry)))
            }
        }
    }

    Ok(blocks)
}

fn deserialize_icons<'de, D>(deserializer: D) -> Result<Map<String, String>, D::Error>
where
    D: Deserializer<'de>,
{
    map_type!(Icons, String;
              s => Ok(Icons(icons::get_icons(s).ok_or_else(|| "cannot find specified icons")?)));

    deserializer.deserialize_any(MapType::<Icons, String>(PhantomData, PhantomData))
}

fn deserialize_themes<'de, D>(deserializer: D) -> Result<Theme, D::Error>
where
    D: Deserializer<'de>,
{
    ThemeConfig::deserialize(deserializer)?
        .into_theme()
        .ok_or_else(|| D::Error::custom("Unrecognized theme name."))
}

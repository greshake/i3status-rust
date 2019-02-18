use de::*;
use icons;
use serde::de::{self, Deserialize, Deserializer};
use std::collections::HashMap as Map;
use std::marker::PhantomData;
use std::ops::Deref;
use std::str::FromStr;
use themes::{self, Theme};
use toml::value;

#[derive(Deserialize, Debug, Clone)]
pub struct Config {
    #[serde(default = "icons::default", deserialize_with = "deserialize_icons")]
    pub icons: Map<String, String>,
    #[serde(default = "themes::default", deserialize_with = "deserialize_themes")]
    pub theme: Theme,
    #[serde(rename = "block", deserialize_with = "deserialize_blocks")]
    pub blocks: Vec<(String, value::Value)>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            icons: icons::default(),
            theme: themes::default(),
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
    map_type!(ThemeIntermediary, String;
              s => Ok(ThemeIntermediary(themes::get_theme(s).ok_or_else(|| "cannot find specified theme")?.owned_map())));

    let intermediary: Map<String, String> = deserializer.deserialize_any(MapType::<ThemeIntermediary, String>(PhantomData, PhantomData))?;

    Deserialize::deserialize(de::value::MapDeserializer::new(intermediary.into_iter()))
}

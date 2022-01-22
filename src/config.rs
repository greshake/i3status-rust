use serde::de::{Deserialize, Deserializer};
use serde_derive::Deserialize;
use smartstring::alias::String;
use std::sync::Arc;
use toml::value;

use crate::blocks::BlockType;
use crate::icons::Icons;
use crate::themes::Theme;

#[derive(Deserialize, Debug, Clone)]
pub struct SharedConfig {
    #[serde(default)]
    pub theme: Arc<Theme>,
    #[serde(default)]
    pub icons: Arc<Icons>,
    #[serde(default = "Config::default_icons_format")]
    pub icons_format: Arc<String>,
}

impl SharedConfig {
    pub fn get_icon(&self, icon: &str) -> crate::errors::Result<String> {
        use crate::errors::OptionExt;
        Ok(self.icons_format.replace(
            "{icon}",
            self.icons
                .0
                .get(icon)
                .error(format!("Icon '{}' not found: please check your icons file or open a new issue on GitHub if you use precompiled icons", icon))?,
        ).into())
    }
}

impl Default for SharedConfig {
    fn default() -> Self {
        Self {
            theme: Arc::new(Theme::default()),
            icons: Arc::new(Icons::default()),
            icons_format: Arc::new(" {icon} ".into()),
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct Config {
    #[serde(flatten)]
    pub shared: SharedConfig,

    /// Set to `true` to invert mouse wheel direction
    #[serde(default)]
    pub invert_scrolling: bool,

    /// The maximum delay (ms) between two clicks that are considered as doulble click
    #[serde(default = "Config::default_double_click_delay")]
    pub double_click_delay: u64,

    #[serde(deserialize_with = "deserialize_blocks")]
    pub block: Vec<(BlockType, value::Value)>,
}

impl Config {
    fn default_icons_format() -> Arc<String> {
        Arc::new(" {icon} ".into())
    }

    fn default_double_click_delay() -> u64 {
        200
    }
}

fn deserialize_blocks<'de, D>(deserializer: D) -> Result<Vec<(BlockType, value::Value)>, D::Error>
where
    D: Deserializer<'de>,
{
    let mut blocks: Vec<(BlockType, value::Value)> = Vec::new();
    let raw_blocks: Vec<value::Table> = Deserialize::deserialize(deserializer)?;
    for mut entry in raw_blocks {
        if let Some(name) = entry.remove("block") {
            let name_str = name.to_string();
            let block = BlockType::deserialize(name)
                .map_err(|_| serde::de::Error::custom(format!("Unknown block '{}'", name_str)))?;
            blocks.push((block, value::Value::Table(entry)));
        }
    }

    Ok(blocks)
}

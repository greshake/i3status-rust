use serde::{Deserialize, Deserializer};
use smart_default::SmartDefault;
use std::collections::HashMap;
use std::sync::Arc;

use crate::blocks::BlockConfig;
use crate::click::ClickHandler;
use crate::errors::*;
use crate::formatting::config::Config as FormatConfig;
use crate::icons::{Icon, Icons};
use crate::themes::{Theme, ThemeOverrides, ThemeUserConfig};

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(default)]
pub struct Config {
    #[serde(flatten)]
    pub shared: SharedConfig,

    /// Set to `true` to invert mouse wheel direction
    pub invert_scrolling: bool,

    /// The maximum delay (ms) between two clicks that are considered as double click
    pub double_click_delay: u64,

    #[default(" {$short_error_message|X} ".parse().unwrap())]
    pub error_format: FormatConfig,
    #[default(" $full_error_message ".parse().unwrap())]
    pub error_fullscreen_format: FormatConfig,

    #[serde(rename = "block")]
    pub blocks: Vec<BlockConfigEntry>,
}

#[derive(Deserialize, Debug, Clone, SmartDefault)]
#[serde(default)]
pub struct SharedConfig {
    #[serde(deserialize_with = "deserialize_theme_config")]
    pub theme: Arc<Theme>,
    pub icons: Arc<Icons>,
    #[default(Arc::new("{icon}".into()))]
    pub icons_format: Arc<String>,
}

impl SharedConfig {
    pub fn get_icon(&self, icon: &str, value: Option<f64>) -> Option<String> {
        if icon.is_empty() {
            Some(String::new())
        } else {
            Some(
                self.icons_format
                    .replace("{icon}", self.icons.get(icon, value)?),
            )
        }
    }
}

#[derive(Deserialize, Debug)]
pub struct BlockConfigEntry {
    #[serde(flatten)]
    pub common: CommonBlockConfig,
    #[serde(flatten)]
    pub config: BlockConfig,
}

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(default)]
pub struct CommonBlockConfig {
    pub click: ClickHandler,
    pub signal: Option<i32>,
    pub icons_format: Option<String>,
    pub theme_overrides: Option<ThemeOverrides>,
    pub icons_overrides: Option<HashMap<String, Icon>>,
    pub merge_with_next: bool,

    #[default(5)]
    pub error_interval: u64,
    pub error_format: FormatConfig,
    pub error_fullscreen_format: FormatConfig,

    pub if_command: Option<String>,
}

fn deserialize_theme_config<'de, D>(deserializer: D) -> Result<Arc<Theme>, D::Error>
where
    D: Deserializer<'de>,
{
    let theme_config = ThemeUserConfig::deserialize(deserializer)?;
    let theme = Theme::try_from(theme_config).serde_error()?;
    Ok(Arc::new(theme))
}

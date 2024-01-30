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

#[derive(Deserialize, Debug)]
pub struct Config {
    #[serde(flatten)]
    pub shared: SharedConfig,

    /// Set to `true` to invert mouse wheel direction
    #[serde(default)]
    pub invert_scrolling: bool,

    /// The maximum delay (ms) between two clicks that are considered as double click
    #[serde(default)]
    pub double_click_delay: u64,

    #[serde(default = "default_error_format")]
    pub error_format: FormatConfig,
    #[serde(default = "default_error_fullscreen")]
    pub error_fullscreen_format: FormatConfig,

    #[serde(default)]
    #[serde(rename = "block")]
    pub blocks: Vec<BlockConfigEntry>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct SharedConfig {
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_theme_config")]
    pub theme: Arc<Theme>,
    #[serde(default)]
    pub icons: Arc<Icons>,
    #[serde(default = "default_icons_format")]
    pub icons_format: Arc<String>,
}

impl Default for SharedConfig {
    fn default() -> Self {
        Self {
            theme: Default::default(),
            icons: Default::default(),
            icons_format: default_icons_format(),
        }
    }
}

fn default_error_format() -> FormatConfig {
    " {$short_error_message|X} ".parse().unwrap()
}

fn default_error_fullscreen() -> FormatConfig {
    " $full_error_message ".parse().unwrap()
}

fn default_icons_format() -> Arc<String> {
    Arc::new("{icon}".into())
}

impl SharedConfig {
    pub fn get_icon(&self, icon: &str, value: Option<f64>) -> Result<String> {
        if icon.is_empty() {
            Ok(String::new())
        } else {
            Ok(self.icons_format.replace(
                "{icon}",
                self.icons
                    .get(icon, value)
                    .or_error(|| format!("Icon '{icon}' not found"))?,
            ))
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

use serde::{Deserialize, Deserializer};
use std::sync::Arc;
use toml::value;

use crate::errors::*;
use crate::icons::Icons;
use crate::themes::{Theme, ThemeUserConfig};
use crate::util::default;

#[derive(Deserialize, Debug, Clone)]
pub struct SharedConfig {
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_theme_config")]
    pub theme: Arc<Theme>,
    #[serde(default)]
    pub icons: Arc<Icons>,
    #[serde(default = "Config::default_icons_format")]
    pub icons_format: Arc<String>,
}

impl SharedConfig {
    pub fn get_icon(&self, icon: &str) -> Option<String> {
        if icon.is_empty() {
            Some(String::new())
        } else {
            Some(self.icons_format.replace("{icon}", self.icons.0.get(icon)?))
        }
    }
}

impl Default for SharedConfig {
    fn default() -> Self {
        Self {
            theme: default(),
            icons: default(),
            icons_format: Arc::new("{icon}".into()),
        }
    }
}

#[derive(Deserialize, Debug)]
pub struct Config {
    #[serde(flatten)]
    pub shared: SharedConfig,

    /// Set to `true` to invert mouse wheel direction
    #[serde(default)]
    pub invert_scrolling: bool,

    /// The maximum delay (ms) between two clicks that are considered as doulble click
    #[serde(default)]
    pub double_click_delay: u64,

    #[serde(default = "Config::default_error_format")]
    pub error_format: String,
    #[serde(default = "Config::default_error_fullscreen_format")]
    pub error_fullscreen_format: String,

    #[serde(rename = "block")]
    pub blocks: Vec<value::Value>,
}

impl Config {
    fn default_icons_format() -> Arc<String> {
        Arc::new("{icon}".into())
    }

    fn default_error_format() -> String {
        " {$short_error_message|X} ".into()
    }

    fn default_error_fullscreen_format() -> String {
        " $full_error_message ".into()
    }
}

fn deserialize_theme_config<'de, D>(deserializer: D) -> Result<Arc<Theme>, D::Error>
where
    D: Deserializer<'de>,
{
    let theme_config = ThemeUserConfig::deserialize(deserializer)?;
    let theme = Theme::try_from(theme_config).serde_error()?;
    Ok(Arc::new(theme))
}

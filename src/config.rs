use serde_derive::Deserialize;
use smartstring::alias::String;
use std::sync::Arc;
use toml::value;

use crate::errors::*;
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
    pub fn get_icon(&self, icon: &str) -> Result<String> {
        Ok(self.icons_format.replace(
            "{icon}",
            self.icons
                .0
                .get(icon)
                .error(format!("Icon '{icon}' not found: please check your icons file or open a new issue on GitHub if you use precompiled icons"))?,
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

    #[serde(rename = "block")]
    pub blocks: Vec<value::Value>,
}

impl Config {
    fn default_icons_format() -> Arc<String> {
        Arc::new(" {icon} ".into())
    }

    fn default_double_click_delay() -> u64 {
        200
    }
}

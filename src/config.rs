use std::collections::HashMap as Map;
use std::marker::PhantomData;
use std::ops::Deref;
use std::path::Path;
use std::str::FromStr;

use serde::de::{Deserialize, Deserializer, Error};
use serde_derive::Deserialize;
use toml::value;

use crate::de::*;
use crate::input::MouseButton;
use crate::themes::{Theme, ThemeConfig};
use crate::util::deserialize_file;
use crate::{errors, icons};

#[derive(Deserialize, Debug, Clone)]
pub struct Config {
    #[serde(default = "icons::default", deserialize_with = "deserialize_icons")]
    pub icons: Map<String, String>,
    #[serde(deserialize_with = "deserialize_themes")]
    pub theme: Theme,
    /// Direction of scrolling, "natural" or "reverse".
    ///
    /// Configuring natural scrolling on input devices changes the way i3status-rust
    /// processes mouse wheel events: pushing the wheen away now is interpreted as downward
    /// motion which is undesired for sliders. Use "natural" to invert this.
    #[serde(default = "Scrolling::default", rename = "scrolling")]
    pub scrolling: Scrolling,
    #[serde(rename = "block", deserialize_with = "deserialize_blocks")]
    pub blocks: Vec<(String, value::Value)>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            icons: icons::default(),
            theme: Theme::default(),
            scrolling: Scrolling::default(),
            blocks: Vec::new(),
        }
    }
}

impl From<LegacyConfig> for Config {
    fn from(legacy_config: LegacyConfig) -> Self {
        Config {
            icons: legacy_config.icons,
            theme: legacy_config
                .theme
                .and_then(|s| Theme::from_name(s.as_str()))
                .unwrap_or_default(),
            scrolling: legacy_config.scrolling,
            blocks: legacy_config.blocks,
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct LegacyConfig {
    #[serde(default = "icons::default", deserialize_with = "deserialize_icons")]
    pub icons: Map<String, String>,
    #[serde(default)]
    pub theme: Option<String>,
    /// Direction of scrolling, "natural" or "reverse".
    ///
    /// Configuring natural scrolling on input devices changes the way i3status-rust
    /// processes mouse wheel events: pushing the wheen away now is interpreted as downward
    /// motion which is undesired for sliders. Use "natural" to invert this.
    #[serde(default = "Scrolling::default", rename = "scrolling")]
    pub scrolling: Scrolling,
    #[serde(rename = "block", deserialize_with = "deserialize_blocks")]
    pub blocks: Vec<(String, value::Value)>,
}

impl Default for LegacyConfig {
    fn default() -> Self {
        LegacyConfig {
            icons: icons::default(),
            theme: None,
            scrolling: Scrolling::default(),
            blocks: Vec::new(),
        }
    }
}

#[derive(Deserialize, Copy, Clone, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Scrolling {
    Reverse,
    Natural,
}

#[derive(Copy, Clone, Debug)]
pub enum LogicalDirection {
    Up,
    Down,
}

impl Scrolling {
    pub fn to_logical_direction(self, button: MouseButton) -> Option<LogicalDirection> {
        use LogicalDirection::*;
        use MouseButton::*;
        use Scrolling::*;
        match (self, button) {
            (Reverse, WheelUp) | (Natural, WheelDown) => Some(Up),
            (Reverse, WheelDown) | (Natural, WheelUp) => Some(Down),
            _ => None,
        }
    }
}

impl Default for Scrolling {
    fn default() -> Self {
        Scrolling::Reverse
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
              s => Ok(Icons(icons::get_icons(s).ok_or(format!("cannot find icon set called '{}'", s))?)));

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

// this function may belong somewhere else...
pub fn load_config(config_path: &Path) -> errors::Result<Config> {
    let config: errors::Result<Config> = deserialize_file(config_path.to_str().unwrap());
    config.or_else(|_| {
        let legacy_config: errors::Result<LegacyConfig> =
            deserialize_file(config_path.to_str().unwrap());
        legacy_config.map(|legacy| legacy.into())
    })
}
#[cfg(test)]
mod tests {
    use crate::config::load_config;
    use assert_fs::prelude::{FileWriteStr, PathChild};
    use assert_fs::TempDir;

    #[test]
    fn test_load_config_legacy() {
        let temp_dir = TempDir::new().unwrap();
        let config_file_path = temp_dir.child("status.toml");
        config_file_path
            .write_str(
                concat!(
                    "icons = \"awesome\"\n",
                    "theme = \"solarized-dark\"\n",
                    "[[block]]\n",
                    "block = \"load\"\n",
                    "interval = 1\n",
                    "format = \"{1m}\"",
                )
                .as_ref(),
            )
            .unwrap();
        let config = load_config(config_file_path.path());
        config.unwrap();
    }

    #[test]
    fn test_load_config() {
        let temp_dir = TempDir::new().unwrap();
        let config_file_path = temp_dir.child("status.toml");
        config_file_path
            .write_str(
                concat!(
                    "icons = \"awesome\"\n",
                    "[theme]\n",
                    "name = \"solarized-dark\"\n",
                    "[[block]]\n",
                    "block = \"load\"\n",
                    "interval = 1\n",
                    "format = \"{1m}\"",
                )
                .as_ref(),
            )
            .unwrap();
        let config = load_config(config_file_path.path());
        config.unwrap();
    }
}

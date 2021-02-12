use std::collections::HashMap as Map;
use std::marker::PhantomData;
use std::ops::Deref;
use std::str::FromStr;

use serde::de::{Deserialize, Deserializer};
use serde_derive::Deserialize;
use toml::value;

use crate::de::*;
use crate::icons;
use crate::input::MouseButton;
use crate::themes::Theme;

#[derive(Deserialize, Debug, Clone)]
pub struct Config {
    #[serde(default = "icons::default", deserialize_with = "deserialize_icons")]
    pub icons: Map<String, String>,
    #[serde(default = "Theme::default")]
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

#[cfg(test)]
mod tests {
    use crate::config::Config;
    use crate::util::deserialize_file;
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
        let config: Result<Config, _> = deserialize_file(config_file_path.path());
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
        let config: Result<Config, _> = deserialize_file(config_file_path.path());
        config.unwrap();
    }
}

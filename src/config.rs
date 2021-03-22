use std::collections::HashMap;
use std::rc::Rc;

use serde::de::{Deserialize, Deserializer};
use serde_derive::Deserialize;
use toml::value;

use crate::errors;
use crate::icons::Icons;
use crate::protocol::i3bar_event::MouseButton;
use crate::themes::Theme;

#[derive(Debug)]
pub struct SharedConfig {
    pub theme: Rc<Theme>,
    icons: Rc<Icons>,
    icons_format: String,
    pub scrolling: Scrolling,
}

impl SharedConfig {
    pub fn new(config: &Config) -> Self {
        Self {
            theme: Rc::new(config.theme.clone()),
            icons: Rc::new(config.icons.clone()),
            icons_format: config.icons_format.clone(),
            scrolling: config.scrolling,
        }
    }

    pub fn icons_format_override(&mut self, icons_format: String) {
        self.icons_format = icons_format;
    }

    pub fn theme_override(&mut self, overrides: &HashMap<String, String>) -> errors::Result<()> {
        let mut theme = self.theme.as_ref().clone();
        for entry in overrides {
            match entry.0.as_str() {
                "idle_fg" => theme.idle_fg = Some(entry.1.to_string()),
                "idle_bg" => theme.idle_bg = Some(entry.1.to_string()),
                "info_fg" => theme.info_fg = Some(entry.1.to_string()),
                "info_bg" => theme.info_bg = Some(entry.1.to_string()),
                "good_fg" => theme.good_fg = Some(entry.1.to_string()),
                "good_bg" => theme.good_bg = Some(entry.1.to_string()),
                "warning_fg" => theme.warning_fg = Some(entry.1.to_string()),
                "warning_bg" => theme.warning_bg = Some(entry.1.to_string()),
                "critical_fg" => theme.critical_fg = Some(entry.1.to_string()),
                "critical_bg" => theme.critical_bg = Some(entry.1.to_string()),
                x => {
                    return Err(errors::ConfigurationError(
                        format!("Theme element \"{}\" cannot be overriden", x),
                        String::new(),
                    ))
                }
            }
        }
        self.theme = Rc::new(theme);
        Ok(())
    }

    pub fn get_icon(&self, icon: &str) -> crate::errors::Result<String> {
        use crate::errors::OptionExt;
        Ok(self.icons_format.clone().replace(
            "{icon}",
            self.icons
                .0
                .get(icon)
                .internal_error("get_icon()", &format!("icon '{}' not found: please check your icon file or open a new issue on GitHub if you use a precompiled icons.", icon))?,
        ))
    }
}

impl Default for SharedConfig {
    fn default() -> Self {
        Self {
            theme: Rc::new(Theme::default()),
            icons: Rc::new(Icons::default()),
            icons_format: " {icon} ".to_string(),
            scrolling: Scrolling::default(),
        }
    }
}

impl Clone for SharedConfig {
    fn clone(&self) -> Self {
        Self {
            theme: Rc::clone(&self.theme),
            icons: Rc::clone(&self.icons),
            icons_format: self.icons_format.clone(),
            scrolling: self.scrolling,
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct Config {
    #[serde(default)]
    pub icons: Icons,

    #[serde(default)]
    pub theme: Theme,

    #[serde(default = "Config::default_icons_format")]
    pub icons_format: String,

    /// Direction of scrolling, "natural" or "reverse".
    ///
    /// Configuring natural scrolling on input devices changes the way i3status-rust
    /// processes mouse wheel events: pushing the wheen away now is interpreted as downward
    /// motion which is undesired for sliders. Use "natural" to invert this.
    #[serde(default)]
    pub scrolling: Scrolling,

    #[serde(rename = "block", deserialize_with = "deserialize_blocks")]
    pub blocks: Vec<(String, value::Value)>,
}

impl Config {
    fn default_icons_format() -> String {
        " {icon} ".to_string()
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            icons: Icons::default(),
            theme: Theme::default(),
            icons_format: Config::default_icons_format(),
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

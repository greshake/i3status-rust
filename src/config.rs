use serde::{Deserialize, Deserializer};
use smart_default::SmartDefault;
use std::collections::HashMap;
use std::os::unix::process::parent_id;
use std::path::Path;
use std::sync::Arc;

use crate::blocks::BlockConfig;
use crate::click::ClickHandler;
use crate::errors::*;
use crate::formatting::config::Config as FormatConfig;
use crate::geolocator::Geolocator;
use crate::icons::{Icon, Icons};
use crate::themes::color::Color;
use crate::themes::{Theme, ThemeOverrides, ThemeUserConfig};
use crate::util::read_file;

#[derive(Deserialize, Debug, Default)]
#[serde(default, deny_unknown_fields)]
pub struct SwayIntegration {
    pub use_sway_bar_colors: bool,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(flatten)]
    pub shared: SharedConfig,

    /// Set to `true` to invert mouse wheel direction
    #[serde(default)]
    pub invert_scrolling: bool,

    #[serde(default)]
    pub geolocator: Arc<Geolocator>,

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

    #[serde(default)]
    pub sway_integration: SwayIntegration,
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
    " {$restart_block_icon |}{$short_error_message|X} "
        .parse()
        .unwrap()
}

fn default_error_fullscreen() -> FormatConfig {
    " {$restart_block_icon |}$full_error_message.str(w:170,rot_interval:0.2) "
        .parse()
        .unwrap()
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
    pub max_retries: Option<u8>,

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

pub struct SwayBarColors {
    pub background: Color,
    pub statusline: Color,
    pub separator: Color,
}

pub async fn try_parse_sway_bar_colors() -> Result<SwayBarColors> {
    let mut swayipc_connection = swayipc_async::Connection::new()
        .await
        .error("Failed to open swayipc connection")?;

    let mut current_pid = parent_id().to_string();
    while current_pid != "1" {
        let cmdline = read_file(Path::new("/proc").join(&current_pid).join("cmdline"))
            .await
            .or_error(|| format!("Failed to read /proc/{current_pid}/cmdline"))?;
        let cmdline_parts: Vec<_> = cmdline.split('\0').collect();
        if let Some(bar_id) = cmdline_parts
            .iter()
            .position(|&s| s == "-b" || s == "--bar_id")
            .and_then(|pos| cmdline_parts.get(pos + 1))
        {
            let bar_config = swayipc_connection
                .get_bar_config(&bar_id)
                .await
                .error("Failed to get swaybar config")?;
            return Ok(SwayBarColors {
                background: bar_config
                    .colors
                    .background
                    .parse()
                    .error("Failed to parse background color")?,
                statusline: bar_config
                    .colors
                    .statusline
                    .parse()
                    .error("Failed to parse statusline color")?,
                separator: bar_config
                    .colors
                    .separator
                    .parse()
                    .error("Failed to parse separator color")?,
            });
        }
        let stat = read_file(Path::new("/proc").join(&current_pid).join("stat"))
            .await
            .or_error(|| format!("Failed to read /proc/{current_pid}/stat"))?;

        current_pid = stat
            .split(' ')
            .nth(3)
            .unwrap()
            .parse()
            .or_error(|| format!("Failed to parse parent PID from /proc/{current_pid}/stat"))?;
    }
    Err(Error::new(
        "Unable to find swaybar process in parent process tree",
    ))
}

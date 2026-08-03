use serde::{Deserialize, Deserializer};
use smart_default::SmartDefault;
use std::collections::HashMap;
use std::sync::Arc;

use crate::blocks::BlockConfig;
use crate::click::ClickHandler;
use crate::errors::*;
use crate::formatting::config::Config as FormatConfig;
use crate::geolocator::Geolocator;
use crate::icons::{Icon, Icons};
use crate::themes::{Theme, ThemeOverrides, ThemeUserConfig};

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

/// Doctor-only: when enabled (inside a doctor worker process, never in the
/// real bar), every icon that actually renders is recorded as (name,
/// produced string). Only icons on the format branch that succeeded pass
/// through `get_icon`, and the produced string tells the live-output
/// analysis which characters came from an icon.
///
/// Worker-local by construction: a worker renders one block, so the
/// recording belongs to that render.
static ICON_RECORDER: std::sync::OnceLock<std::sync::Mutex<Vec<(String, String)>>> =
    std::sync::OnceLock::new();

pub(crate) fn enable_icon_recorder() {
    let _ = ICON_RECORDER.set(std::sync::Mutex::new(Vec::new()));
}

/// The icons recorded so far, clearing the record.
pub(crate) fn take_recorded_icons() -> Vec<(String, String)> {
    ICON_RECORDER
        .get()
        .and_then(|recorder| recorder.lock().ok().map(|mut r| std::mem::take(&mut *r)))
        .unwrap_or_default()
}

fn record_icon(icon: &str, produced: &str) {
    if let Some(recorder) = ICON_RECORDER.get()
        && let Ok(mut recorder) = recorder.lock()
    {
        recorder.push((icon.to_string(), produced.to_string()));
    }
}

impl SharedConfig {
    pub fn get_icon(&self, icon: &str, value: Option<f64>) -> Result<String> {
        if icon.is_empty() {
            return Ok(String::new());
        }
        let produced = self.icons_format.replace(
            "{icon}",
            self.icons
                .get(icon, value)
                .or_error(|| format!("Icon '{icon}' not found"))?,
        );
        record_icon(icon, &produced);
        Ok(produced)
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

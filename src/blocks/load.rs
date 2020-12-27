use std::collections::BTreeMap;
use std::fs::{read_to_string, OpenOptions};
use std::io::prelude::*;
use std::time::Duration;

use crossbeam_channel::Sender;
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::Config;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::scheduler::Task;
use crate::util::{pseudo_uuid, FormatTemplate};
use crate::widget::{I3BarWidget, State};
use crate::widgets::text::TextWidget;

pub struct Load {
    text: TextWidget,
    logical_cores: u32,
    format: FormatTemplate,
    id: String,
    update_interval: Duration,
    minimum_info: f32,
    minimum_warning: f32,
    minimum_critical: f32,
    minimum_visible: [f32; 3],
    visible: bool,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct LoadConfig {
    #[serde(default = "LoadConfig::default_format")]
    pub format: String,
    #[serde(
        default = "LoadConfig::default_interval",
        deserialize_with = "deserialize_duration"
    )]
    pub interval: Duration,

    /// Minimum load, where state is set to info
    #[serde(default = "LoadConfig::default_info")]
    pub info: f32,

    /// Minimum load, where state is set to warning
    #[serde(default = "LoadConfig::default_warning")]
    pub warning: f32,

    /// Minimum load, where state is set to critical
    #[serde(default = "LoadConfig::default_critical")]
    pub critical: f32,

    /// Minimum values for 1m, 5m and 15m respectively for the Load block to display
    #[serde(default = "LoadConfig::default_min_visible")]
    pub minimum_visible: [f32; 3],

    #[serde(default = "LoadConfig::default_color_overrides")]
    pub color_overrides: Option<BTreeMap<String, String>>,
}

impl LoadConfig {
    fn default_format() -> String {
        "{1m}".to_owned()
    }

    fn default_interval() -> Duration {
        Duration::from_secs(5)
    }

    fn default_info() -> f32 {
        0.3
    }

    fn default_warning() -> f32 {
        0.6
    }

    fn default_critical() -> f32 {
        0.9
    }

    fn default_min_visible() -> [f32; 3] {
        [0.0, 0.0, 0.0]
    }

    fn default_color_overrides() -> Option<BTreeMap<String, String>> {
        None
    }
}

impl ConfigBlock for Load {
    type Config = LoadConfig;

    fn new(
        block_config: Self::Config,
        config: Config,
        _tx_update_request: Sender<Task>,
    ) -> Result<Self> {
        let text = TextWidget::new(config)
            .with_icon("cogs")
            .with_state(State::Info);

        // borrowed from https://docs.rs/cpuinfo/0.1.1/src/cpuinfo/count/logical.rs.html#4-6
        let content = read_to_string("/proc/cpuinfo")
            .block_error("load", "Your system doesn't support /proc/cpuinfo")?;
        let logical_cores = content
            .lines()
            .filter(|l| l.starts_with("processor"))
            .count() as u32;

        Ok(Load {
            id: pseudo_uuid(),
            logical_cores,
            update_interval: block_config.interval,
            minimum_info: block_config.info,
            minimum_warning: block_config.warning,
            minimum_critical: block_config.critical,
            minimum_visible: block_config.minimum_visible,
            visible: true,
            format: FormatTemplate::from_string(&block_config.format)
                .block_error("load", "Invalid format specified for load")?,
            text,
        })
    }
}

impl Block for Load {
    fn update(&mut self) -> Result<Option<Update>> {
        let mut f = OpenOptions::new()
            .read(true)
            .open("/proc/loadavg")
            .block_error(
                "load",
                "Your system does not support reading the load average from /proc/loadavg",
            )?;
        let mut loadavg = String::new();
        f.read_to_string(&mut loadavg)
            .block_error("load", "Failed to read the load average of your system!")?;

        let split: Vec<&str> = (&loadavg).split(' ').collect();

        let values = map!("{1m}" => split[0],
                          "{5m}" => split[1],
                          "{15m}" => split[2]);

        /* Parse all the loadavg values. The block is
         * visible if they are all greater than their min
         * in self.minimum_visible */
        let parsed: Vec<f32> = split
            .iter()
            .take(3)
            .map(|v| v.parse::<f32>().unwrap_or(f32::INFINITY))
            .collect();

        self.visible = self
            .minimum_visible
            .iter()
            .zip(parsed.iter())
            .all(|(min, v)| v >= min);

        let used_perc = parsed[0] / self.logical_cores as f32;

        self.text.set_state(match used_perc {
            x if x > self.minimum_critical => State::Critical,
            x if x > self.minimum_warning => State::Warning,
            x if x > self.minimum_info => State::Info,
            _ => State::Idle,
        });

        self.text.set_text(self.format.render_static_str(&values)?);

        Ok(Some(self.update_interval.into()))
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        if !self.visible {
            return Vec::new();
        }

        vec![&self.text]
    }

    fn id(&self) -> &str {
        &self.id
    }
}

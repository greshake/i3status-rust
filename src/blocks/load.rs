use std::fs::{read_to_string, OpenOptions};
use std::io::prelude::*;
use std::time::Duration;

use crossbeam_channel::Sender;
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::SharedConfig;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::formatting::value::Value;
use crate::formatting::FormatTemplate;
use crate::scheduler::Task;
use crate::widgets::text::TextWidget;
use crate::widgets::{I3BarWidget, State};

pub struct Load {
    id: usize,
    text: TextWidget,
    logical_cores: u32,
    format: FormatTemplate,
    update_interval: Duration,
    minimum_info: f64,
    minimum_warning: f64,
    minimum_critical: f64,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct LoadConfig {
    pub format: FormatTemplate,
    #[serde(deserialize_with = "deserialize_duration")]
    pub interval: Duration,

    /// Minimum load, where state is set to info
    pub info: f64,

    /// Minimum load, where state is set to warning
    pub warning: f64,

    /// Minimum load, where state is set to critical
    pub critical: f64,
}

impl Default for LoadConfig {
    fn default() -> Self {
        Self {
            format: FormatTemplate::default(),
            interval: Duration::from_secs(5),
            info: 0.3,
            warning: 0.6,
            critical: 0.9,
        }
    }
}

impl ConfigBlock for Load {
    type Config = LoadConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        shared_config: SharedConfig,
        _tx_update_request: Sender<Task>,
    ) -> Result<Self> {
        let text = TextWidget::new(id, 0, shared_config)
            .with_icon("cogs")?
            .with_state(State::Info);

        // borrowed from https://docs.rs/cpuinfo/0.1.1/src/cpuinfo/count/logical.rs.html#4-6
        let content = read_to_string("/proc/cpuinfo")
            .block_error("load", "Your system doesn't support /proc/cpuinfo")?;
        let logical_cores = content
            .lines()
            .filter(|l| l.starts_with("processor"))
            .count() as u32;

        Ok(Load {
            id,
            logical_cores,
            update_interval: block_config.interval,
            minimum_info: block_config.info,
            minimum_warning: block_config.warning,
            minimum_critical: block_config.critical,
            format: block_config.format.with_default("{1m}")?,
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

        let split: Vec<f64> = loadavg
            .split(' ')
            .take(3)
            .map(|x| x.parse().unwrap())
            .collect();

        let values = map!(
            "1m" => Value::from_float(split[0]),
            "5m" => Value::from_float(split[1]),
            "15m" => Value::from_float(split[2]),
        );

        let used_perc = split[0] / (self.logical_cores as f64);

        self.text.set_state(match used_perc {
            x if x > self.minimum_critical => State::Critical,
            x if x > self.minimum_warning => State::Warning,
            x if x > self.minimum_info => State::Info,
            _ => State::Idle,
        });

        self.text.set_texts(self.format.render(&values)?);

        Ok(Some(self.update_interval.into()))
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.text]
    }

    fn id(&self) -> usize {
        self.id
    }
}

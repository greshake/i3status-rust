use std::time::Duration;

use crate::blocks::{Block, ConfigBlock};
use crate::config::Config;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::scheduler::Task;
use crate::util::FormatTemplate;
use crate::widget::{I3BarWidget, State};
use crate::widgets::text::TextWidget;
use crossbeam_channel::Sender;

use std::fs::{File, OpenOptions};
use std::io::prelude::*;
use std::io::BufReader;

use uuid::Uuid;

#[derive(Serialize)]
struct LoadValues {
    la_1m: f32,
    la_5m: f32,
    la_15m: f32,
}

pub struct Load {
    text: TextWidget,
    logical_cores: u32,
    format: FormatTemplate,
    id: String,
    update_interval: Duration,
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
}

impl LoadConfig {
    fn default_format() -> String {
        "{1m}".to_owned()
    }

    fn default_interval() -> Duration {
        Duration::from_secs(5)
    }
}

impl ConfigBlock for Load {
    type Config = LoadConfig;

    fn new(
        block_config: Self::Config,
        config: Config,
        _tx_update_request: Sender<Task>,
    ) -> Result<Self> {
        let format = block_config
            .format
            .replace("{1m}", "{la_1m}")
            .replace("{5m}", "{la_5m}")
            .replace("{15m}", "{la_15m}");
        let text = TextWidget::new(config)
            .with_icon("cogs")
            .with_state(State::Info);

        let f = File::open("/proc/cpuinfo")
            .block_error("load", "Your system doesn't support /proc/cpuinfo")?;
        let f = BufReader::new(f);

        let mut logical_cores = 0;

        for line in f.lines().scan((), |_, x| x.ok()) {
            // TODO: Does this value always represent the correct number of logical cores?
            if line.starts_with("siblings") {
                let split: Vec<&str> = (&line).split(' ').collect();
                logical_cores = split[1]
                    .parse::<u32>()
                    .block_error("load", "Invalid Cpu info format!")?;
                break;
            }
        }

        Ok(Load {
            id: Uuid::new_v4().simple().to_string(),
            logical_cores,
            update_interval: block_config.interval,
            format: FormatTemplate::from_string(&format)?,
            text,
        })
    }
}

impl Block for Load {
    fn update(&mut self) -> Result<Option<Duration>> {
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

        let values = LoadValues {
            la_1m: split[0]
                .parse::<f32>()
                .block_error("load", "failed to parse float percentage")?,
            la_5m: split[1]
                .parse::<f32>()
                .block_error("load", "failed to parse float percentage")?,
            la_15m: split[2]
                .parse::<f32>()
                .block_error("load", "failed to parse float percentage")?,
        };

        let used_perc = values.la_1m / self.logical_cores as f32;
        self.text
            .set_state(match_range!(used_perc, default: (State::Idle) {
                    0.0 ; 0.3 => State::Idle,
                    0.3 ; 0.6 => State::Info,
                    0.6 ; 0.9 => State::Warning
            }));

        self.text.set_text(self.format.render(&values));

        Ok(Some(self.update_interval))
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.text]
    }

    fn id(&self) -> &str {
        &self.id
    }
}

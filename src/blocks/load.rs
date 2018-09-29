use std::time::Duration;

use block::{Block, ConfigBlock};
use config::Config;
use de::deserialize_duration;
use errors::*;
use widgets::text::TextWidget;
use widget::{I3BarWidget, State};
use util::FormatTemplate;
use chan::Sender;
use scheduler::Task;

use std::io::BufReader;
use std::io::prelude::*;
use std::fs::{File, OpenOptions};

use uuid::Uuid;

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
    #[serde(default = "LoadConfig::default_interval", deserialize_with = "deserialize_duration")]
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

    fn new(block_config: Self::Config, config: Config, _tx_update_request: Sender<Task>) -> Result<Self> {
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
            id: format!("{}", Uuid::new_v4().to_simple()),
            logical_cores: logical_cores,
            update_interval: block_config.interval,
            format: FormatTemplate::from_string(block_config.format)
                .block_error("load", "Invalid format specified for load")?,
            text: text,
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

        let values = map!("{1m}" => split[0],
                          "{5m}" => split[1],
                          "{15m}" => split[2]);

        let used_perc = values["{1m}"]
            .parse::<f32>()
            .block_error("load", "failed to parse float percentage")? / self.logical_cores as f32;
        self.text.set_state(
            match_range!(used_perc, default: (State::Idle) {
                0.0 ; 0.3 => State::Idle,
                0.3 ; 0.6 => State::Info,
                0.6 ; 0.9 => State::Warning
        }),
        );

        self.text.set_text(self.format.render_static_str(&values)?);

        Ok(Some(self.update_interval))
    }

    fn view(&self) -> Vec<&I3BarWidget> {
        vec![&self.text]
    }

    fn id(&self) -> &str {
        &self.id
    }
}

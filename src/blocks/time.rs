extern crate chrono;

use std::time::Duration;

use block::{Block, ConfigBlock};
use config::Config;
use de::deserialize_duration;
use errors::*;
use self::chrono::offset::local::Local;
use scheduler::Task;
use std::sync::mpsc::Sender;
use widgets::text::TextWidget;
use widget::I3BarWidget;
use uuid::Uuid;

pub struct Time {
    time: TextWidget,
    id: String,
    update_interval: Duration,
    format: String,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct TimeConfig {
    /// Format string.<br/> See [chrono docs](https://docs.rs/chrono/0.3.0/chrono/format/strftime/index.html#specifiers) for all options.
    #[serde(default = "TimeConfig::default_format")]
    pub format: String,

    /// Update interval in seconds
    #[serde(default = "TimeConfig::default_interval", deserialize_with = "deserialize_duration")]
    pub interval: Duration,
}

impl TimeConfig {
    fn default_format() -> String {
        "%a %d/%m %R".to_owned()
    }

    fn default_interval() -> Duration {
        Duration::from_secs(5)
    }
}

impl ConfigBlock for Time {
    type Config = TimeConfig;

    fn new(block_config: Self::Config, config: Config, _tx_update_request: Sender<Task>) -> Result<Self> {
        Ok(Time {
            id: Uuid::new_v4().simple().to_string(),
            format: block_config.format,
            time: TextWidget::new(config).with_text("").with_icon("time"),
            update_interval: block_config.interval,
        })
    }
}

impl Block for Time {
    fn update(&mut self) -> Result<Option<Duration>> {
        self.time.set_text(
            format!("{}", Local::now().format(&self.format)),
        );
        Ok(Some(self.update_interval))
    }

    fn view(&self) -> Vec<&I3BarWidget> {
        vec![&self.time]
    }

    fn id(&self) -> &str {
        &self.id
    }
}

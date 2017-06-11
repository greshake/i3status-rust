extern crate chrono;

use std::time::Duration;

use block::Block;
use config::Config;
use self::chrono::offset::local::Local;
use widgets::text::TextWidget;
use widget::{I3BarWidget};
use toml::value::Value;
use uuid::Uuid;


pub struct Time {
    time: TextWidget,
    id: String,
    update_interval: Duration,
    format: String
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct TimeConfig {
    /// Format string.<br/> See [chrono docs](https://docs.rs/chrono/0.3.0/chrono/format/strftime/index.html#specifiers) for all options.
    #[serde(default = "TimeConfig::default_format")]
    pub format: String,

    /// Update interval in seconds
    #[serde(default = "TimeConfig::default_interval")]
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

impl Time {
    pub fn new(block_config: Value, config: Config) -> Time {
        Time {
            id: Uuid::new_v4().simple().to_string(),
            format: get_str_default!(block_config, "format", "%a %d/%m %R"),
            time: TextWidget::new(config).with_text("").with_icon("time"),
            update_interval: Duration::new(get_u64_default!(block_config, "interval", 5), 0),
        }
    }
}


impl Block for Time {
    fn update(&mut self) -> Option<Duration> {
        self.time.set_text(format!("{}", Local::now().format(&self.format)));
        Some(self.update_interval.clone())
    }

    fn view(&self) -> Vec<&I3BarWidget> {
        vec![&self.time]
    }

    fn id(&self) -> &str {
        &self.id
    }
}

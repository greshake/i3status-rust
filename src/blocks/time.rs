use std::time::Duration;
use std::process::Command;
use std::ffi::OsStr;

use block::{Block, ConfigBlock};
use config::Config;
use de::{deserialize_duration, deserialize_timezone};
use errors::*;
use chrono::offset::{Utc, Local};
use chrono_tz::Tz;
use scheduler::Task;
use chan::Sender;
use widgets::button::ButtonWidget;
use widget::I3BarWidget;
use input::I3BarEvent;
use uuid::Uuid;

pub struct Time {
    time: ButtonWidget,
    id: String,
    update_interval: Duration,
    format: String,
    on_click: Option<String>,
    timezone: Option<Tz>,
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

    #[serde(default = "TimeConfig::default_on_click")]
    pub on_click: Option<String>,

    #[serde(default = "TimeConfig::default_timezone", deserialize_with = "deserialize_timezone")]
    pub timezone: Option<Tz>,
}

impl TimeConfig {
    fn default_format() -> String {
        "%a %d/%m %R".to_owned()
    }

    fn default_interval() -> Duration {
        Duration::from_secs(5)
    }

    fn default_on_click() -> Option<String> {
        None
    }

    fn default_timezone() -> Option<Tz> {
        None
    }
}

impl ConfigBlock for Time {
    type Config = TimeConfig;

    fn new(block_config: Self::Config, config: Config, _tx_update_request: Sender<Task>) -> Result<Self> {
        let i = format!("{}", Uuid::new_v4().to_simple());
        Ok(Time {
            id: i.clone(),
            format: block_config.format,
            time: ButtonWidget::new(config, i.as_str())
                .with_text("")
                .with_icon("time"),
            update_interval: block_config.interval,
            on_click: block_config.on_click,
            timezone: block_config.timezone,
        })
    }
}

impl Block for Time {
    fn update(&mut self) -> Result<Option<Duration>> {
        let time = match self.timezone {
            Some(tz) => Utc::now().with_timezone(&tz).format(&self.format),
            None => Local::now().format(&self.format),
        };
        self.time.set_text(format!("{}", time));
        Ok(Some(self.update_interval))
    }


    fn click(&mut self, e: &I3BarEvent) -> Result<()> {
        let mut command = "".to_string();
        if self.on_click.is_some() {
            command = self.on_click.clone().unwrap();
        }


        if let Some(ref name) = e.name {
            if name.as_str() == self.id && self.on_click.is_some() {
                let command_broken: Vec<&str> = command.split_whitespace().collect();
                let mut itr = command_broken.iter();
                let mut _cmd = Command::new(OsStr::new(&itr.next().unwrap()))
                    .args(itr)
                    .spawn();
            }
        }
        Ok(())
    }

    fn view(&self) -> Vec<&I3BarWidget> {
        vec![&self.time]
    }

    fn id(&self) -> &str {
        &self.id
    }
}

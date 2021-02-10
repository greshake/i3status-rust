use std::collections::BTreeMap;
use std::convert::TryInto;
use std::time::Duration;

use chrono::{
    offset::{Local, Utc},
    Locale,
};
use chrono_tz::Tz;
use crossbeam_channel::Sender;
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::Config;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::scheduler::Task;
use crate::widget::I3BarWidget;
use crate::widgets::button::ButtonWidget;

pub struct Time {
    time: ButtonWidget,
    id: usize,
    update_interval: Duration,
    format: String,
    timezone: Option<Tz>,
    locale: Option<String>,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct TimeConfig {
    /// Format string.<br/> See [chrono docs](https://docs.rs/chrono/0.3.0/chrono/format/strftime/index.html#specifiers) for all options.
    #[serde(default = "TimeConfig::default_format")]
    pub format: String,

    /// Update interval in seconds
    #[serde(
        default = "TimeConfig::default_interval",
        deserialize_with = "deserialize_duration"
    )]
    pub interval: Duration,

    #[serde(default = "TimeConfig::default_timezone")]
    pub timezone: Option<Tz>,

    #[serde(default = "TimeConfig::default_locale")]
    pub locale: Option<String>,

    #[serde(default = "TimeConfig::default_color_overrides")]
    pub color_overrides: Option<BTreeMap<String, String>>,
}

impl TimeConfig {
    fn default_format() -> String {
        "%a %d/%m %R".to_owned()
    }

    fn default_interval() -> Duration {
        Duration::from_secs(5)
    }

    fn default_timezone() -> Option<Tz> {
        None
    }

    fn default_locale() -> Option<String> {
        None
    }

    fn default_color_overrides() -> Option<BTreeMap<String, String>> {
        None
    }
}

impl ConfigBlock for Time {
    type Config = TimeConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        config: Config,
        _tx_update_request: Sender<Task>,
    ) -> Result<Self> {
        Ok(Time {
            id,
            format: block_config.format,
            time: ButtonWidget::new(config, id)
                .with_text("")
                .with_icon("time"),
            update_interval: block_config.interval,
            timezone: block_config.timezone,
            locale: block_config.locale,
        })
    }
}

impl Block for Time {
    fn update(&mut self) -> Result<Option<Update>> {
        let time = match &self.locale {
            Some(l) => {
                let locale: Locale = l
                    .as_str()
                    .try_into()
                    .block_error("time", "invalid locale")?;
                match self.timezone {
                    Some(tz) => Utc::now()
                        .with_timezone(&tz)
                        .format_localized(&self.format, locale),
                    None => Local::now().format_localized(&self.format, locale),
                }
            }
            None => match self.timezone {
                Some(tz) => Utc::now().with_timezone(&tz).format(&self.format),
                None => Local::now().format(&self.format),
            },
        };
        self.time.set_text(format!("{}", time));
        Ok(Some(self.update_interval.into()))
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.time]
    }

    fn id(&self) -> usize {
        self.id
    }
}

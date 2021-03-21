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
use crate::config::SharedConfig;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::scheduler::Task;
use crate::widgets::text::TextWidget;
use crate::widgets::I3BarWidget;

pub struct Time {
    id: usize,
    time: TextWidget,
    update_interval: Duration,
    format: String,
    timezone: Option<Tz>,
    locale: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct TimeConfig {
    /// Format string.
    ///
    /// See [chrono docs](https://docs.rs/chrono/0.3.0/chrono/format/strftime/index.html#specifiers) for all options.
    pub format: String,

    /// Update interval in seconds
    #[serde(deserialize_with = "deserialize_duration")]
    pub interval: Duration,

    pub timezone: Option<Tz>,

    pub locale: Option<String>,
}

impl Default for TimeConfig {
    fn default() -> Self {
        Self {
            format: "%a %d/%m %R".to_string(),
            interval: Duration::from_secs(5),
            timezone: None,
            locale: None,
        }
    }
}

impl ConfigBlock for Time {
    type Config = TimeConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        shared_config: SharedConfig,
        _tx_update_request: Sender<Task>,
    ) -> Result<Self> {
        Ok(Time {
            id,
            time: TextWidget::new(id, 0, shared_config)
                .with_text("")
                .with_icon("time")?,
            update_interval: block_config.interval,
            format: block_config.format,
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

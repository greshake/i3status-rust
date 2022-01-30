use std::collections::HashMap;
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
use crate::formatting::FormatTemplate;
use crate::scheduler::Task;
use crate::widgets::text::TextWidget;
use crate::widgets::I3BarWidget;

pub struct Time {
    id: usize,
    time: TextWidget,
    update_interval: Duration,
    formats: (String, Option<String>),
    timezone: Option<Tz>,
    locale: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct TimeConfig {
    /// Format string.
    ///
    /// See [chrono docs](https://docs.rs/chrono/0.3.0/chrono/format/strftime/index.html#specifiers) for all options.
    pub format: FormatTemplate,

    /// Update interval in seconds
    #[serde(deserialize_with = "deserialize_duration")]
    pub interval: Duration,

    pub timezone: Option<Tz>,

    pub locale: Option<String>,
}

impl Default for TimeConfig {
    fn default() -> Self {
        Self {
            format: FormatTemplate::default(),
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
            formats: block_config
                .format
                .with_default("%a %d/%m %R")?
                .render(&HashMap::<&str, _>::new())?,
            timezone: block_config.timezone,
            locale: block_config.locale,
        })
    }
}

impl Time {
    fn get_formatted_time(&self, format: &str) -> Result<String> {
        let time = match &self.locale {
            Some(l) => {
                let locale: Locale = l
                    .as_str()
                    .try_into()
                    .block_error("time", "invalid locale")?;
                match self.timezone {
                    Some(tz) => Utc::now()
                        .with_timezone(&tz)
                        .format_localized(format, locale),
                    None => Local::now().format_localized(format, locale),
                }
            }
            None => match self.timezone {
                Some(tz) => Utc::now().with_timezone(&tz).format(format),
                None => Local::now().format(format),
            },
        };
        Ok(format!("{}", time))
    }
}

impl Block for Time {
    fn update(&mut self) -> Result<Option<Update>> {
        if self.timezone.is_none() {
            // Update timezone because `chrono` will not do that for us.
            // https://github.com/chronotope/chrono/issues/272
            unsafe { tzset() };
        }

        let full = self.get_formatted_time(&self.formats.0)?;
        let short = match &self.formats.1 {
            Some(short_fmt) => Some(self.get_formatted_time(short_fmt)?),
            None => None,
        };
        self.time.set_texts((full, short));
        Ok(Some(self.update_interval.into()))
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.time]
    }

    fn id(&self) -> usize {
        self.id
    }
}

extern "C" {
    /// The tzset function initializes the tzname variable from the value of the TZ environment
    /// variable. It is not usually necessary for your program to call this function, because it is
    /// called automatically when you use the other time conversion functions that depend on the
    /// time zone.
    fn tzset();
}

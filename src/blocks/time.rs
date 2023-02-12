//! The current time.
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `format` | Format string. See [chrono docs](https://docs.rs/chrono/0.3.0/chrono/format/strftime/index.html#specifiers) for all options. | `" $icon $timestamp.datetime() "`
//! `interval` | Update interval in seconds | `10`
//! `timezone` | A timezone specifier (e.g. "Europe/Lisbon") | Local timezone
//!
//! Placeholder   | Value                                       | Type     | Unit
//! --------------|---------------------------------------------|----------|-----
//! `icon`        | A static icon                               | Icon     | -
//! `timestamp`   | The current time                            | Datetime | -
//!
//! # Example
//!
//! ```toml
//! [[block]]
//! block = "time"
//! interval = 60
//! [block.format]
//! full = " $icon $timestamp.datetime(f:'%a %Y-%m-%d %R %Z', l:fr_BE) "
//! short = " $icon $timestamp.datetime(f:%R) "
//! ```
//!
//! # Icons Used
//! - `time`

use chrono::Utc;
use chrono_tz::Tz;

use super::prelude::*;
use crate::formatting::config::DummyConfig;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(default)]
pub struct Config {
    format: DummyConfig,
    #[default(1.into())]
    interval: Seconds,
    timezone: Option<Tz>,
}

pub async fn run(config: Config, mut api: CommonApi) -> Result<()> {
    let mut widget = Widget::new();

    let format = config
        .format
        .full
        .as_deref()
        .unwrap_or(" $icon $timestamp.datetime() ");

    let format_short = config.format.short.as_deref().unwrap_or_default();

    widget.set_format(FormatConfig::default().with_defaults(format, format_short)?);

    let timezone = config.timezone;

    let mut timer = config.interval.timer();

    loop {
        if timezone.is_none() {
            // Update timezone because `chrono` will not do that for us.
            // https://github.com/chronotope/chrono/issues/272
            unsafe { tzset() };
        }

        widget.set_values(map!(
            "icon" => Value::icon(api.get_icon("time")?),
            "timestamp" => Value::datetime(Utc::now(), timezone)
        ));

        api.set_widget(&widget).await?;

        tokio::select! {
            _ = timer.tick() => (),
            _ = api.wait_for_update_request() => (),
        }
    }
}

extern "C" {
    /// The tzset function initializes the tzname variable from the value of the TZ environment
    /// variable. It is not usually necessary for your program to call this function, because it is
    /// called automatically when you use the other time conversion functions that depend on the
    /// time zone.
    fn tzset();
}

//! The current time.
//!
//! # Configuration
//!
//! Key        | Values | Default
//! -----------|--------|--------
//! `format`   | Format string. See [chrono docs](https://docs.rs/chrono/0.3.0/chrono/format/strftime/index.html#specifiers) for all options. | `" $icon $timestamp.datetime() "`
//! `interval` | Update interval in seconds | `10`
//! `timezone` | A timezone specifier (e.g. "Europe/Lisbon") | Local timezone
//!
//! Placeholder   | Value                                       | Type     | Unit
//! --------------|---------------------------------------------|----------|-----
//! `icon`        | A static icon                               | Icon     | -
//! `timestamp`   | The current time                            | Datetime | -
//!
//! Action          | Default button
//! ----------------|---------------
//! `next_timezone` | Left
//! `prev_timezone` | Right
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

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub format: FormatConfig,
    #[default(10.into())]
    pub interval: Seconds,
    pub timezone: Option<Timezone>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum Timezone {
    Timezone(Tz),
    Timezones(Vec<Tz>),
}

pub async fn run(config: &Config, api: &CommonApi) -> Result<()> {
    let mut actions = api.get_actions().await?;
    api.set_default_actions(&[
        (MouseButton::Left, None, "next_timezone"),
        (MouseButton::Right, None, "prev_timezone"),
    ])
    .await?;

    let format = config
        .format
        .with_default(" $icon $timestamp.datetime() ")?;

    let timezones = match config.timezone.clone() {
        Some(tzs) => match tzs {
            Timezone::Timezone(tz) => vec![tz],
            Timezone::Timezones(tzs) => tzs,
        },
        None => Vec::new(),
    };

    let prev_step_length = timezones.len().saturating_sub(2);

    let mut timezone_iter = timezones.iter().cycle();

    let mut timezone = timezone_iter.next();

    let mut timer = config.interval.timer();

    loop {
        if timezone.is_none() {
            // Update timezone because `chrono` will not do that for us.
            // https://github.com/chronotope/chrono/issues/272
            unsafe { tzset() };
        }

        let mut widget = Widget::new().with_format(format.clone());

        widget.set_values(map! {
            "icon" => Value::icon("time"),
            "timestamp" => Value::datetime(Utc::now(), timezone.copied())
        });

        api.set_widget(widget).await?;

        tokio::select! {
            _ = timer.tick() => (),
            _ = api.wait_for_update_request() => (),
            Some(action) = actions.recv() => match action.as_ref() {
                "next_timezone" => {
                    timezone = timezone_iter.next();
                },
                "prev_timezone" => {
                    timezone = timezone_iter.nth(prev_step_length);
                },
                _ => (),
            }
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

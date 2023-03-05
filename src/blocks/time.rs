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
//! Action   | Default button
//! ---------|---------------
//! `next_timezone` | Left
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
#[serde(default)]
pub struct Config {
    format: FormatConfig,
    #[default(1.into())]
    interval: Seconds,
    timezone: Option<Timezone>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum Timezone {
    Timezone(Tz),
    Timezones(Vec<Tz>),
}

pub async fn run(config: Config, mut api: CommonApi) -> Result<()> {
    // "next_timezone" changes the current displayed timezone to the timezone next in the list.
    api.set_default_actions(&[(MouseButton::Left, None, "next_timezone")])
        .await?;

    let mut widget = Widget::new().with_format(
        config
            .format
            .with_default(" $icon $timestamp.datetime() ")?,
    );

    let timezones = match config.timezone {
        Some(tzs) => match tzs {
            Timezone::Timezone(tz) => vec![tz],
            Timezone::Timezones(tzs) => tzs,
        },
        None => Vec::new(),
    };

    let mut timezone_iter = timezones.iter().cycle();

    let mut timezone = timezone_iter.next();

    let mut timer = config.interval.timer();

    loop {
        if timezone.is_none() {
            // Update timezone because `chrono` will not do that for us.
            // https://github.com/chronotope/chrono/issues/272
            unsafe { tzset() };
        }

        widget.set_values(map!(
            "icon" => Value::icon(api.get_icon("time")?),
            "timestamp" => Value::datetime(Utc::now(), timezone.copied())
        ));

        api.set_widget(&widget).await?;

        tokio::select! {
            _ = timer.tick() => (),
            event = api.event() => {
                match event {
                    Action(e) => if e == "next_timezone" {
                       timezone = timezone_iter.next();
                    },
                    UpdateRequest => {}
                }
            },
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

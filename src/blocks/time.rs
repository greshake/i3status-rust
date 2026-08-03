//! The current time.
//!
//! # Configuration
//!
//! Key        | Values | Default
//! -----------|--------|--------
//! `format`   | [MultiFormat][MaybeMultiFormatConfig] string. See [chrono docs](https://docs.rs/chrono/0.3.0/chrono/format/strftime/index.html#specifiers) for all options. | `[" $icon $timestamp.datetime() "]`
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
//! `next_format`   | -
//! `prev_format`   | -
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
//! # Non Gregorian calendars
//!
//! You can use calendars other than the Gregorian calendar by adding the calendar specifier in the locale string. When using
//! this feature you can't use chrono style format string, and you should use one of the options provided by
//! the `icu4x` crate: `short`, `medium`, `long`, `full`.
//! If you set `precision` to `hours`/`hour`/`h`, `minutes`/`minute`/`m`, or `seconds`/`second`/`s` then then the datetime will be formatted accordingly, otherwise only the date will be displayed.
//!
//! ** Only available using feature `icu_calendar`. **
//!
//! ## Example
//!
//! ```toml
//! [[block]]
//! block = "time"
//! interval = 60
//! format = "$timestamp.datetime(locale:'fa-IR-u-ca-persian', f:'full', precision: minutes)"
//! ```
//!
//! # Icons Used
//! - `time`

use chrono::{Timelike as _, Utc};
use chrono_tz::Tz;

use super::prelude::*;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(default)]
pub struct Config {
    #[serde(flatten)]
    pub formats: MaybeMultiFormatConfig,
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

pub(crate) fn prepare(config: &Config) -> Result<Arc<BlockPlan>> {
    BlockPlan::new(vec![
        OutputPlan::new(
            "main",
            config
                .format
                .with_default(" $icon $timestamp.datetime() ")?,
        )
        .icon("icon", IconChoices::one("time")),
    ])
}

pub async fn run(config: &Config, api: &CommonApi, plan: &Arc<BlockPlan>) -> Result<()> {
    let mut actions = api.get_actions()?;
    api.set_default_actions(&[
        (MouseButton::Left, None, "next_timezone"),
        (MouseButton::Right, None, "prev_timezone"),
    ])?;

    let output_main = plan.output("main")?;

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

    let interval_seconds = config.interval.seconds().max(1);

    let mut timer = tokio::time::interval_at(
        tokio::time::Instant::now() + Duration::from_secs(interval_seconds),
        Duration::from_secs(interval_seconds),
    );
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        let mut widget = output_main.new_widget();
        let now = Utc::now();

        widget.set_values(map! {
            "icon" => output_main.icon_value("icon")?,
            "timestamp" => Value::datetime(now, timezone.copied())
        });

        api.set_widget(widget)?;

        let phase = now.second() as u64 % interval_seconds;
        if phase != 0 {
            timer.reset_after(Duration::from_secs(interval_seconds - phase));
        }

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
                "next_format" => {
                    formats.next_format();
                },
                "prev_format" => {
                   formats.prev_format();
                },
                _ => (),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_declares_single_output_with_time_icon() {
        let plan = prepare(&Config::default()).unwrap();
        let ids: Vec<_> = plan.outputs().map(|o| o.id()).collect();
        assert_eq!(ids, ["main"]);
        let main = plan.output("main").unwrap();
        assert_eq!(main.single_icon("icon").unwrap(), "time");
        assert!(main.format().contains_key("timestamp"));
    }

    #[test]
    fn plan_uses_configured_format() {
        let config = Config {
            format: " $timestamp.datetime(f:%R) ".parse().unwrap(),
            ..Config::default()
        };
        let plan = prepare(&config).unwrap();
        let main = plan.output("main").unwrap();
        assert!(main.format().contains_key("timestamp"));
        assert!(!main.format().contains_key("icon"));
    }
}

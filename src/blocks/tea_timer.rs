//! Timer
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `format` | A string to customise the output of this block. See below for available placeholders. | `" $icon {$minutes:$seconds |}"`
//! `increment` | The numbers of seconds to add each time the block is clicked. | 30
//!
//! Placeholder      | Value                                                          | Type   | Unit
//! -----------------|----------------------------------------------------------------|--------|---------------
//! `icon`           | A static icon                                                  | Icon   | -
//! `hours`          | The hours remaining on the timer                               | Text   | h
//! `minutes`        | The minutes remaining on the timer                             | Text   | mn
//! `seconds`        | The seconds remaining on the timer                             | Text   | s
//!
//! `hours`, `minutes`, and `seconds` are unset when the timer is inactive.
//!
//! # Example
//!
//! ```toml
//! [[block]]
//! block = "tea_timer"
//! format = " $icon {$minutes:$seconds |}"
//! ```
//!
//! # Icons Used
//! - `tea`

use super::prelude::*;
use chrono::{Duration, Utc};

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    format: FormatConfig,
    increment: Option<i64>,
}

pub async fn run(config: Config, mut api: CommonApi) -> Result<()> {
    let interval: Seconds = 1.into();
    let mut timer = interval.timer();

    let format = config.format.with_default(" $icon {$minutes:$seconds |}")?;
    let mut widget = Widget::new().with_format(format);

    let increment = Duration::seconds(config.increment.unwrap_or(30));
    let mut timer_end = Utc::now();

    loop {
        let remaining_time = timer_end - Utc::now();
        let is_timer_active = remaining_time > Duration::zero();

        let (hours, minutes, seconds) = if is_timer_active {
            (
                remaining_time.num_hours(),
                remaining_time.num_minutes() % 60,
                remaining_time.num_seconds() % 60,
            )
        } else {
            (0, 0, 0)
        };

        widget.set_values(map!(
            "icon" => Value::icon(api.get_icon("tea")?),
            [if is_timer_active] "hours" => Value::text(format!("{:02}", hours)),
            [if is_timer_active] "minutes" => Value::text(format!("{:02}", minutes)),
            [if is_timer_active] "seconds" => Value::text(format!("{:02}", seconds)),
        ));

        api.set_widget(&widget).await?;

        tokio::select! {
            _ = timer.tick(), if is_timer_active => (),
            event = api.event() => match event {
                UpdateRequest => (),
                Click(click) => {
                    let now = Utc::now();

                    match click.button {
                        MouseButton::Left if is_timer_active => timer_end += increment,
                        MouseButton::Left => timer_end = now + increment,
                        MouseButton::Right => timer_end = now,
                        _ => {}
                    }
                },
            }
        }
    }
}

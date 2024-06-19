//! Timer
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `format` | A string to customise the output of this block. See below for available placeholders. | <code>\" $icon {$time.duration(hms:true) \|}\"</code>
//! `increment` | The numbers of seconds to add each time the block is clicked. | 30
//! `done_cmd` | A command to run in `sh` when timer finishes. | None
//!
//! Placeholder            | Value                                                          | Type     | Unit
//! -----------------------|----------------------------------------------------------------|----------|---------------
//! `icon`                 | A static icon                                                  | Icon     | -
//! `time`                 | The time remaining on the timer                                | Duration | -
//! `hours` *DEPRECATED*   | The hours remaining on the timer                               | Text     | h
//! `minutes` *DEPRECATED* | The minutes remaining on the timer                             | Text     | mn
//! `seconds` *DEPRECATED* | The seconds remaining on the timer                             | Text     | s
//!
//! `time`, `hours`, `minutes`, and `seconds` are unset when the timer is inactive.
//!
//! `hours`, `minutes`, and `seconds` have been deprecated in favor of `time`.
//!
//! Action      | Default button
//! ------------|---------------
//! `increment` | Left / Wheel Up
//! `decrement` | Wheel Down
//! `reset`     | Right
//!
//! # Example
//!
//! ```toml
//! [[block]]
//! block = "tea_timer"
//! format = " $icon {$minutes:$seconds |}"
//! done_cmd = "notify-send 'Timer Finished'"
//! ```
//!
//! # Icons Used
//! - `tea`

use super::prelude::*;
use crate::subprocess::spawn_shell;

use std::time::{Duration, Instant};

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub format: FormatConfig,
    pub increment: Option<u64>,
    pub done_cmd: Option<String>,
}

pub async fn run(config: &Config, api: &CommonApi) -> Result<()> {
    let mut actions = api.get_actions()?;
    api.set_default_actions(&[
        (MouseButton::Left, None, "increment"),
        (MouseButton::WheelUp, None, "increment"),
        (MouseButton::WheelDown, None, "decrement"),
        (MouseButton::Right, None, "reset"),
    ])?;

    let interval: Seconds = 1.into();
    let mut timer = interval.timer();

    let format = config
        .format
        .with_default(" $icon {$time.duration(hms:true) |}")?;

    let increment = Duration::from_secs(config.increment.unwrap_or(30));
    let mut timer_end = Instant::now();

    let mut timer_was_active = false;

    loop {
        let mut widget = Widget::new().with_format(format.clone());

        let remaining_time = timer_end - Instant::now();
        let is_timer_active = !remaining_time.is_zero();

        if !is_timer_active && timer_was_active {
            if let Some(cmd) = &config.done_cmd {
                spawn_shell(cmd).error("done_cmd error")?;
            }
        }
        timer_was_active = is_timer_active;

        let mut values = map!(
            "icon" => Value::icon("tea"),
        );

        if is_timer_active {
            values.insert("time".into(), Value::duration(remaining_time));
            let mut seconds = remaining_time.as_secs();

            if format.contains_key("hours") {
                let hours = seconds / 3_600;
                values.insert("hours".into(), Value::text(format!("{hours:02}")));
                seconds %= 3_600;
            }

            if format.contains_key("minutes") {
                let minutes = seconds / 60;
                values.insert("minutes".into(), Value::text(format!("{minutes:02}")));
                seconds %= 60;
            }

            values.insert("seconds".into(), Value::text(format!("{seconds:02}")));
        }

        widget.set_values(values);

        api.set_widget(widget)?;

        select! {
            _ = timer.tick(), if is_timer_active => (),
            _ = api.wait_for_update_request() => (),
            Some(action) = actions.recv() => {
                let now = Instant::now();
                match action.as_ref() {
                    "increment" if is_timer_active => timer_end += increment,
                    "increment" => timer_end = now + increment,
                    "decrement" if is_timer_active => timer_end -= increment,
                    "reset" => timer_end = now,
                    _ => (),
                }
            }
        }
    }
}

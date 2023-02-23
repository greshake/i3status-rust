//! Watson statistics
//!
//! [Watson](http://tailordev.github.io/Watson/) is a simple CLI time tracking application. This block will show the name of your current active project, tags and optionally recorded time. Clicking the widget will toggle the `show_time` variable dynamically.
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `format` | A string to customise the output of this block. See below for available placeholders | `" $text |"`
//! `show_time` | Whether to show recorded time. | `false`
//! `state_path` | Path to the Watson state file. Supports path expansions e.g. `~`. | `$XDG_CONFIG_HOME/watson/state`
//! `interval` | Update interval, in seconds. | `60`
//!
//! Placeholder   | Value                   | Type   | Unit
//! --------------|-------------------------|--------|-----
//! `text`        | Current activity        | Text   | -
//!
//! Action             | Description                     | Default button
//! -------------------|---------------------------------|---------------
//! `toggle_show_time` | Toggle the value of `show_time` | Left
//!
//! # Example
//!
//! ```toml
//! [[block]]
//! block = "watson"
//! show_time = true
//! state_path = "~/.config/watson/state"
//! ```
//!
//! # TODO
//! - Extend functionality: start / stop watson using this block

use chrono::{offset::Local, DateTime};
use dirs::config_dir;
use inotify::{Inotify, WatchMask};
use serde::de::Deserializer;
use std::path::PathBuf;
use tokio::fs::read_to_string;

use super::prelude::*;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(default)]
pub struct Config {
    format: FormatConfig,
    state_path: Option<ShellString>,
    #[default(60.into())]
    interval: Seconds,
    show_time: bool,
}

pub async fn run(config: Config, mut api: CommonApi) -> Result<()> {
    api.set_default_actions(&[(MouseButton::Left, None, "toggle_show_time")])
        .await?;

    let mut widget = Widget::new().with_format(config.format.with_default(" $text |")?);

    let mut show_time = config.show_time;

    let (state_dir, state_file, state_path) = match config.state_path {
        Some(p) => {
            let mut p: PathBuf = (*p.expand()?).into();
            let path = p.clone();
            let file = p.file_name().error("Failed to parse state_dir")?.to_owned();
            p.pop();
            (p, file, path)
        }
        None => {
            let mut path = config_dir().error("xdg config directory not found")?;
            path.push("watson");
            let dir = path.clone();
            path.push("state");
            (dir, "state".into(), path)
        }
    };

    let mut notify = Inotify::init().error("Failed to start inotify")?;
    notify
        .add_watch(&state_dir, WatchMask::CREATE | WatchMask::MOVED_TO)
        .error("Failed to watch watson state directory")?;
    let mut state_updates = notify
        .event_stream([0; 1024])
        .error("Failed to create event stream")?;

    let mut timer = config.interval.timer();
    let mut prev_state = None;

    loop {
        let state = read_to_string(&state_path)
            .await
            .error("Failed to read state file")?;
        let state = serde_json::from_str(&state).error("Unable to deserialize state")?;
        match state {
            state @ WatsonState::Active { .. } => {
                widget.state = State::Good;
                widget.set_values(map!(
                  "text" => Value::text(state.format(show_time, "started", format_delta_past))
                ));
                prev_state = Some(state);
            }
            WatsonState::Idle {} => {
                if let Some(prev @ WatsonState::Active { .. }) = &prev_state {
                    // The previous state was active, which means that we just now stopped the time
                    // tracking. This means that we could show some statistics.
                    widget.state = State::Idle;
                    widget.set_values(map!(
                      "text" => Value::text(prev.format(true, "stopped", format_delta_after))
                    ));
                } else {
                    // File is empty which means that there is currently no active time tracking,
                    // and the previous state wasn't time tracking neither so we reset the
                    // contents.
                    widget.state = State::Idle;
                    widget.set_values(Values::default());
                }
                prev_state = Some(state);
            }
        }

        api.set_widget(&widget).await?;

        loop {
            select! {
                _ = timer.tick() => break,
                Some(update) = state_updates.next() => {
                    let update = update.error("Bad inotify update")?;
                    if update.name.map(|x| state_file == x).unwrap_or(false) {
                        break;
                    }
                }
                event = api.event() => match event {
                    UpdateRequest => break,
                    Action(a) if a == "toggle_show_time" => {
                        show_time = !show_time;
                        break;
                    }
                    _ => (),
                }
            }
        }
    }
}

fn format_delta_past(delta: &chrono::Duration) -> String {
    let spans = &[
        ("week", delta.num_weeks()),
        ("day", delta.num_days()),
        ("hour", delta.num_hours()),
        ("minute", delta.num_minutes()),
    ];

    spans
        .iter()
        .filter(|&(_, n)| *n != 0)
        .map(|&(label, n)| format!("{n} {label}{} ago", if n > 1 { "s" } else { "" }))
        .next()
        .unwrap_or_else(|| "now".into())
}

fn format_delta_after(delta: &chrono::Duration) -> String {
    let spans = &[
        ("week", delta.num_weeks()),
        ("day", delta.num_days()),
        ("hour", delta.num_hours()),
        ("minute", delta.num_minutes()),
        ("second", delta.num_seconds()),
    ];

    spans
        .iter()
        .find(|&(_, n)| *n != 0)
        .map(|&(label, n)| format!("after {n} {label}{}", if n > 1 { "s" } else { "" }))
        .unwrap_or_else(|| "now".into())
}

#[derive(Deserialize, Clone, Debug)]
#[serde(untagged)]
enum WatsonState {
    Active {
        project: String,
        #[serde(deserialize_with = "deserialize_local_timestamp")]
        start: DateTime<Local>,
        tags: Vec<String>,
    },
    // This matches an empty JSON object
    Idle {},
}

impl WatsonState {
    fn format(&self, show_time: bool, verb: &str, f: fn(&chrono::Duration) -> String) -> String {
        if let WatsonState::Active {
            project,
            start,
            tags,
        } = self
        {
            let mut s = project.clone();
            if let [first, other @ ..] = &tags[..] {
                s.push_str(" [");
                s.push_str(first);
                for tag in other {
                    s.push(' ');
                    s.push_str(tag);
                }
                s.push(']');
            }
            if show_time {
                s.push(' ');
                s.push_str(verb);
                let delta = Local::now() - *start;
                s.push(' ');
                s.push_str(&f(&delta));
            }
            s
        } else {
            panic!("WatsonState::Idle does not have a specified format")
        }
    }
}

pub fn deserialize_local_timestamp<'de, D>(deserializer: D) -> Result<DateTime<Local>, D::Error>
where
    D: Deserializer<'de>,
{
    use chrono::TimeZone;
    i64::deserialize(deserializer).map(|seconds| Local.timestamp_opt(seconds, 0).single().unwrap())
}

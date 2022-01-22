//! Watson statistics
//!
//! [Watson](http://tailordev.github.io/Watson/) is a simple CLI time tracking application. This block will show the name of your current active project, tags and optionally recorded time. Clicking the widget will toggle the `show_time` variable dynamically.
//!
//! # Configuration
//!
//! Key | Values | Required | Default
//! ----|--------|----------|--------
//! `show_time` | Whether to show recorded time. | No | `false`
//! `state_path` | Path to the Watson state file. | No | `$XDG_CONFIG_HOME/watson/state`
//! `interval` | Update interval, in seconds. | No | `60`
//!
//! # Example
//!
//! ```toml
//! [[block]]
//! block = "watson"
//! show_time = true
//! state_path = "/home/user/.config/watson/state"
//! ```
//!
//! # TODO
//! - Extend functionality: start / stop watson using this block

use std::path::PathBuf;
use tokio::fs::read_to_string;

use inotify::{Inotify, WatchMask};

use chrono::offset::Local;
use chrono::DateTime;

use super::prelude::*;
use crate::de::deserialize_local_timestamp;
use crate::util::xdg_config_home;

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
struct WatsonConfig {
    state_path: Option<ShellString>,
    interval: Seconds,
    show_time: bool,
}

impl Default for WatsonConfig {
    fn default() -> Self {
        Self {
            state_path: None,
            interval: Seconds::new(60),
            show_time: false,
        }
    }
}

pub async fn run(config: toml::Value, mut api: CommonApi) -> Result<()> {
    let config = WatsonConfig::deserialize(config).config_error()?;
    let mut events = api.get_events().await?;

    let mut show_time = config.show_time;

    let (state_dir, state_file, state_path) = match config.state_path {
        Some(p) => {
            let mut p: PathBuf = (&*p.expand()?).into();
            let path = p.clone();
            let file = p.file_name().error("Failed to parse state_dir")?.to_owned();
            p.pop();
            (p, file, path)
        }
        None => {
            let mut path = xdg_config_home().error("XDG_CONFIG directory not found")?;
            path.push("watson");
            let dir = path.clone();
            path.push("state");
            (dir, "state".into(), path)
        }
    };

    let mut notify = Inotify::init().error("Failed to start inotify")?;
    let mut buffer = [0; 1024];
    notify
        .add_watch(&state_dir, WatchMask::CREATE)
        .error("Failed to watch watson state directory")?;
    let mut state_updates = notify
        .event_stream(&mut buffer)
        .error("Failed to create event stream")?;

    let mut timer = config.interval.timer();
    let mut prev_state = None;

    loop {
        let state = read_to_string(&state_path)
            .await
            .error("Failed to read state file")?;
        let state = serde_json::from_str(&state).error("Fnable to deserialize state")?;
        match state {
            state @ WatsonState::Active { .. } => {
                api.set_state(State::Good);
                api.set_text(state.format(show_time, "started", format_delta_past));
                prev_state = Some(state);
            }
            WatsonState::Idle {} => {
                if let Some(prev @ WatsonState::Active { .. }) = &prev_state {
                    // The previous state was active, which means that we just now stopped the time
                    // tracking. This means that we could show some statistics.
                    show_time = true;
                    api.set_text(prev.format(true, "stopped", format_delta_after));
                    api.set_state(State::Idle {});
                    prev_state = Some(state);
                } else {
                    // File is empty which means that there is currently no active time tracking,
                    // and the previous state wasn't time tracking neither so we reset the
                    // contents.
                    show_time = false;
                    api.set_state(State::Idle {});
                    api.set_text(String::new());

                    prev_state = Some(state);
                }
            }
        }

        api.flush().await?;

        loop {
            tokio::select! {
                _ = timer.tick() => break,
                Some(update) = state_updates.next() => {
                    let update = update.error("Bad inoify update")?;
                    if update.name.map(|x| state_file == x).unwrap_or(false) {
                        break;
                    }
                }
                Some(BlockEvent::Click(event)) = events.recv() => {
                    if event.button == MouseButton::Left {
                        show_time = !show_time;
                        break;
                    }
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
        .map(|&(label, n)| format!("{} {}{} ago", n, label, if n > 1 { "s" } else { "" }).into())
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
        .map(|&(label, n)| format!("after {} {}{}", n, label, if n > 1 { "s" } else { "" }).into())
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

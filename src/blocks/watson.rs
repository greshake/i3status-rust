use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::SharedConfig;
use crate::de::deserialize_duration;
use crate::de::deserialize_local_timestamp;
use crate::errors::*;
use crate::protocol::i3bar_event::I3BarEvent;
use crate::scheduler::Task;
use crate::util::xdg_config_home;
use crate::widgets::text::TextWidget;
use crate::widgets::{I3BarWidget, State};
use chrono::offset::Local;
use chrono::DateTime;
use crossbeam_channel::Sender;
use inotify::{Inotify, WatchMask};
use serde_derive::Deserialize;

pub struct Watson {
    id: usize,
    text: TextWidget,
    state_path: PathBuf,
    show_time: bool,
    prev_state: Option<WatsonState>,
    update_interval: Duration,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct WatsonConfig {
    /// Path to state of watson
    pub state_path: PathBuf,

    /// Update interval in seconds
    #[serde(deserialize_with = "deserialize_duration")]
    pub interval: Duration,

    /// Show time spent
    pub show_time: bool,
}

impl Default for WatsonConfig {
    fn default() -> Self {
        let mut config_dir = xdg_config_home();
        config_dir.push("watson/state");
        Self {
            state_path: config_dir,
            interval: Duration::from_secs(60),
            show_time: false,
        }
    }
}

impl ConfigBlock for Watson {
    type Config = WatsonConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        shared_config: SharedConfig,
        tx_update_request: Sender<Task>,
    ) -> Result<Self> {
        let watson = Watson {
            id,
            text: TextWidget::new(id, 0, shared_config),
            state_path: block_config.state_path.clone(),
            show_time: block_config.show_time,
            update_interval: block_config.interval,
            prev_state: None,
        };

        // Spin up a thread to watch for changes to the watson state file
        // and schedule an update if needed.
        thread::spawn(move || {
            // Split filepath into filename and parent directory
            let (file_name, parent_dir) = {
                let name = block_config
                    .state_path
                    .file_name()
                    .expect("watson state file had no name")
                    .to_owned();

                let mut s = block_config.state_path;
                s.pop();
                (name, s)
            };
            let mut notify = Inotify::init().expect("failed to start inotify");

            // We have to watch the parent directory because watson never modifies the state file,
            // but rather write to a temporary file, ensures its not corrupted, backups the
            // previous state file and then renames the new state file. This means that we're
            // always looking for `CREATE` events with the name of the state file.
            notify
                .add_watch(&parent_dir, WatchMask::CREATE | WatchMask::MOVED_TO)
                .expect("failed to watch watson state file");

            let mut buffer = [0; 1024];
            loop {
                let events = notify
                    .read_events_blocking(&mut buffer)
                    .expect("error while reading inotify events");

                for _event in events.filter(|e| e.name == Some(&file_name)) {
                    tx_update_request
                        .send(Task {
                            id,
                            update_time: Instant::now(),
                        })
                        .expect("unable to send task from watson watcher");
                }
            }
        });

        Ok(watson)
    }
}

impl Block for Watson {
    fn update(&mut self) -> Result<Option<Update>> {
        let state = {
            let file = BufReader::new(
                File::open(&self.state_path).block_error("watson", "unable to open state file")?,
            );
            serde_json::from_reader(file).block_error("watson", "unable to deserialize state")?
        };

        match state {
            state @ WatsonState::Active { .. } => {
                self.text.set_state(State::Good);
                self.text
                    .set_text(state.format(self.show_time, "started", format_delta_past));

                self.prev_state = Some(state);
                Ok(if self.show_time {
                    // regular updates if time is enabled
                    Some(self.update_interval.into())
                } else {
                    None
                })
            }
            WatsonState::Idle {} => {
                if let Some(prev_state @ WatsonState::Active { .. }) = &self.prev_state {
                    // The previous state was active, which means that we just now stopped the time
                    // tracking. This means that we could show some statistics.
                    self.text
                        .set_text(prev_state.format(true, "stopped", format_delta_after));
                    self.text.set_state(State::Idle);
                    self.prev_state = Some(state);

                    // Show stopped status for some seconds before returning to idle
                    Ok(Some(Duration::from_secs(5).into()))
                } else {
                    // File is empty which means that there is currently no active time tracking,
                    // and the previous state wasn't time tracking neither so we reset the
                    // contents.
                    self.text.set_state(State::Idle);
                    self.text.set_text(String::new());

                    self.prev_state = Some(state);
                    Ok(None)
                }
            }
        }
    }

    fn click(&mut self, _e: &I3BarEvent) -> Result<()> {
        self.show_time = !self.show_time;
        self.update()?;
        Ok(())
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.text]
    }

    fn id(&self) -> usize {
        self.id
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
        .map(|&(label, n)| format!("{} {}{} ago", n, label, if n > 1 { "s" } else { "" }))
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
        .filter(|&(_, n)| *n != 0)
        .map(|&(label, n)| format!("after {} {}{}", n, label, if n > 1 { "s" } else { "" }))
        .next()
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
            let mut s = String::with_capacity(16);
            s.push_str(project);
            if !tags.is_empty() {
                s.push(' ');
                s.push('[');
                s.push_str(&tags.join(" "));
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

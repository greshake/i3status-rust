use std::env;
use std::iter::{Cycle, Peekable};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};
use std::vec;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::SharedConfig;
use crate::de::deserialize_update;
use crate::errors::*;
use crate::protocol::i3bar_event::I3BarEvent;
use crate::scheduler::Task;
use crate::signals::convert_to_valid_signal;
use crate::subprocess::spawn_child_async;
use crate::widgets::text::TextWidget;
use crate::widgets::{I3BarWidget, State};
use crossbeam_channel::Sender;
use inotify::{EventMask, Inotify, WatchMask};
use serde_derive::Deserialize;

pub struct Custom {
    id: usize,
    update_interval: Update,
    output: TextWidget,
    command: Option<String>,
    on_click: Option<String>,
    cycle: Option<Peekable<Cycle<vec::IntoIter<String>>>>,
    signal: Option<i32>,
    tx_update_request: Sender<Task>,
    pub json: bool,
    hide_when_empty: bool,
    is_empty: bool,
    shell: String,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct CustomConfig {
    /// Update interval in seconds
    #[serde(deserialize_with = "deserialize_update")]
    pub interval: Update,

    /// Shell Command to execute & display
    pub command: Option<String>,

    /// Commands to execute and change when the button is clicked
    pub cycle: Option<Vec<String>>,

    /// Signal to update upon reception
    pub signal: Option<i32>,

    /// Files to watch for modifications and trigger update
    pub watch_files: Option<Vec<String>>,

    /// Parse command output if it contains valid bar JSON
    pub json: bool,

    pub hide_when_empty: bool,

    // TODO make a global config option
    pub shell: String,
}

impl Default for CustomConfig {
    fn default() -> Self {
        Self {
            interval: Update::Every(Duration::from_secs(10)),
            command: None,
            cycle: None,
            signal: None,
            watch_files: None,
            json: false,
            hide_when_empty: false,
            shell: env::var("SHELL").unwrap_or_else(|_| "sh".to_owned()),
        }
    }
}

impl ConfigBlock for Custom {
    type Config = CustomConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        shared_config: SharedConfig,
        tx: Sender<Task>,
    ) -> Result<Self> {
        let mut custom = Custom {
            id,
            update_interval: block_config.interval,
            output: TextWidget::new(id, 0, shared_config),
            command: None,
            on_click: None,
            cycle: None,
            signal: None,
            tx_update_request: tx,
            json: block_config.json,
            hide_when_empty: block_config.hide_when_empty,
            is_empty: true,
            shell: block_config.shell,
        };

        if let Some(signal) = block_config.signal {
            // If the signal is not in the valid range we return an error
            custom.signal = Some(convert_to_valid_signal(signal)?);
        };

        if let Some(paths) = block_config.watch_files {
            let tx_inotify = custom.tx_update_request.clone();
            let mut notify = Inotify::init().expect("Failed to start inotify");
            for path in paths {
                let path_expanded = shellexpand::full(&path).map_err(|e| {
                    ConfigurationError(
                        "custom".to_string(),
                        format!("Failed to expand file path {}: {}", &path, e),
                    )
                })?;
                notify
                    .add_watch(&*path_expanded, WatchMask::MODIFY)
                    .map_err(|e| {
                        ConfigurationError(
                            "custom".to_string(),
                            format!("Failed to watch file {}: {}", &path, e),
                        )
                    })?;
            }
            thread::Builder::new()
                .name("custom".into())
                .spawn(move || {
                    let mut buffer = [0; 1024];
                    loop {
                        let mut events = notify
                            .read_events_blocking(&mut buffer)
                            .expect("Error while reading inotify events");

                        if events.any(|event| event.mask.contains(EventMask::MODIFY)) {
                            tx_inotify
                                .send(Task {
                                    id,
                                    update_time: Instant::now(),
                                })
                                .unwrap();
                        }

                        // Avoid update spam.
                        thread::sleep(Duration::from_millis(250))
                    }
                })
                .unwrap();
        };

        if block_config.cycle.is_some() && block_config.command.is_some() {
            return Err(BlockError(
                "custom".to_string(),
                "`command` and `cycle` are mutually exclusive".to_string(),
            ));
        }

        if let Some(cycle) = block_config.cycle {
            custom.cycle = Some(cycle.into_iter().cycle().peekable());
            return Ok(custom);
        };

        if let Some(command) = block_config.command {
            custom.command = Some(command)
        };

        Ok(custom)
    }

    fn override_on_click(&mut self) -> Option<&mut Option<String>> {
        Some(&mut self.on_click)
    }
}

fn default_icon() -> String {
    String::from("")
}

fn default_state() -> State {
    State::Idle
}

#[derive(Deserialize)]
struct Output {
    #[serde(default = "default_icon")]
    icon: String,
    #[serde(default = "default_state")]
    state: State,
    text: String,
}

impl Block for Custom {
    fn update(&mut self) -> Result<Option<Update>> {
        let command_str = self
            .cycle
            .as_mut()
            .map(|c| c.peek().cloned().unwrap_or_else(|| "".to_owned()))
            .or_else(|| self.command.clone())
            .unwrap_or_else(|| "".to_owned());

        let raw_output = match Command::new(&self.shell)
            .args(&["-c", &command_str])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        {
            Ok(output) => output,
            Err(e) => return Err(BlockError("custom".to_string(), e.to_string())),
        };

        if self.json {
            let output: Output = serde_json::from_str(&*raw_output).map_err(|e| {
                BlockError("custom".to_string(), format!("Error parsing JSON: {}", e))
            })?;
            if output.icon.is_empty() {
                self.output.unset_icon();
            } else {
                self.output.set_icon(&output.icon)?;
            }
            self.output.set_state(output.state);
            self.is_empty = output.text.is_empty();
            self.output.set_text(output.text);
        } else {
            self.is_empty = raw_output.is_empty();
            self.output.set_text(raw_output);
        }

        Ok(Some(self.update_interval.clone()))
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        if self.is_empty && self.hide_when_empty {
            vec![]
        } else {
            vec![&self.output]
        }
    }

    fn signal(&mut self, signal: i32) -> Result<()> {
        if let Some(sig) = self.signal {
            if sig == signal {
                self.tx_update_request.send(Task {
                    id: self.id,
                    update_time: Instant::now(),
                })?;
            }
        }
        Ok(())
    }

    fn click(&mut self, _e: &I3BarEvent) -> Result<()> {
        let mut update = false;

        if let Some(ref on_click) = self.on_click {
            spawn_child_async(&self.shell, &["-c", on_click]).ok();
            update = true;
        }

        if let Some(ref mut cycle) = self.cycle {
            cycle.next();
            update = true;
        }

        if update {
            self.tx_update_request.send(Task {
                id: self.id,
                update_time: Instant::now(),
            })?;
        }

        Ok(())
    }

    fn id(&self) -> usize {
        self.id
    }
}

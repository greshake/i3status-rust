use std::collections::BTreeMap;
use std::env;
use std::iter::{Cycle, Peekable};
use std::process::Command;
use std::time::{Duration, Instant};
use std::vec;

use crossbeam_channel::Sender;
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::Config;
use crate::de::deserialize_update;
use crate::errors::*;
use crate::input::I3BarEvent;
use crate::scheduler::Task;
use crate::signals::convert_to_valid_signal;
use crate::subprocess::spawn_child_async;
use crate::widget::{I3BarWidget, State};
use crate::widgets::button::ButtonWidget;

pub struct Custom {
    id: usize,
    update_interval: Update,
    output: ButtonWidget,
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

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct CustomConfig {
    /// Update interval in seconds
    #[serde(
        default = "CustomConfig::default_interval",
        deserialize_with = "deserialize_update"
    )]
    pub interval: Update,

    /// Shell Command to execute & display
    pub command: Option<String>,

    /// Commands to execute and change when the button is clicked
    pub cycle: Option<Vec<String>>,

    /// Signal to update upon reception
    pub signal: Option<i32>,

    /// Parse command output if it contains valid bar JSON
    #[serde(default = "CustomConfig::default_json")]
    pub json: bool,

    #[serde(default = "CustomConfig::hide_when_empty")]
    pub hide_when_empty: bool,

    pub shell: Option<String>,

    #[serde(default = "CustomConfig::default_color_overrides")]
    pub color_overrides: Option<BTreeMap<String, String>>,
}

impl CustomConfig {
    fn default_interval() -> Update {
        Update::Every(Duration::new(10, 0))
    }

    fn default_json() -> bool {
        false
    }

    fn hide_when_empty() -> bool {
        false
    }

    fn default_color_overrides() -> Option<BTreeMap<String, String>> {
        None
    }
}

impl ConfigBlock for Custom {
    type Config = CustomConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        config: Config,
        tx: Sender<Task>,
    ) -> Result<Self> {
        let mut custom = Custom {
            id,
            update_interval: block_config.interval,
            output: ButtonWidget::new(config, id),
            command: None,
            on_click: None,
            cycle: None,
            signal: None,
            tx_update_request: tx,
            json: block_config.json,
            hide_when_empty: block_config.hide_when_empty,
            is_empty: true,
            shell: if let Some(s) = block_config.shell {
                s
            } else {
                env::var("SHELL").unwrap_or_else(|_| "sh".to_owned())
            },
        };

        if let Some(signal) = block_config.signal {
            // If the signal is not in the valid range we return an error
            custom.signal = Some(convert_to_valid_signal(signal)?);
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

        let raw_output = Command::new(&self.shell)
            .args(&["-c", &command_str])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
            .unwrap_or_else(|e| e.to_string());

        if self.json {
            let output: Output = serde_json::from_str(&*raw_output).map_err(|e| {
                BlockError("custom".to_string(), format!("Error parsing JSON: {}", e))
            })?;
            self.output.set_icon(&output.icon);
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

    fn click(&mut self, event: &I3BarEvent) -> Result<()> {
        if event.matches_id(self.id) {
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
        }

        Ok(())
    }

    fn id(&self) -> usize {
        self.id
    }
}

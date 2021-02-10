use std::collections::BTreeMap;
use std::env;
use std::process::Command;
use std::time::Duration;

use crossbeam_channel::Sender;
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::Config;
use crate::de::deserialize_opt_duration;
use crate::errors::*;
use crate::input::I3BarEvent;
use crate::scheduler::Task;
use crate::widget::{I3BarWidget, State};
use crate::widgets::button::ButtonWidget;

pub struct Toggle {
    id: usize,
    text: ButtonWidget,
    command_on: String,
    command_off: String,
    command_state: String,
    icon_on: String,
    icon_off: String,
    update_interval: Option<Duration>,
    toggled: bool,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct ToggleConfig {
    /// Update interval in seconds
    #[serde(default, deserialize_with = "deserialize_opt_duration")]
    pub interval: Option<Duration>,

    /// Shell Command to enable the toggle
    pub command_on: String,

    /// Shell Command to disable the toggle
    pub command_off: String,

    /// Shell Command to determine toggle state. <br/>Empty output => off. Any output => on.
    pub command_state: String,

    /// Icon ID when toggled on (default is "toggle_on")
    #[serde(default = "ToggleConfig::default_icon_on")]
    pub icon_on: String,

    /// Icon ID when toggled off (default is "toggle_off")
    #[serde(default = "ToggleConfig::default_icon_off")]
    pub icon_off: String,

    /// Text to display in i3bar for this block
    pub text: Option<String>,
    #[serde(default = "ToggleConfig::default_color_overrides")]
    pub color_overrides: Option<BTreeMap<String, String>>,
}

impl ToggleConfig {
    fn default_icon_on() -> String {
        "toggle_on".to_owned()
    }

    fn default_icon_off() -> String {
        "toggle_off".to_owned()
    }
    fn default_color_overrides() -> Option<BTreeMap<String, String>> {
        None
    }
}

impl ConfigBlock for Toggle {
    type Config = ToggleConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        config: Config,
        _tx_update_request: Sender<Task>,
    ) -> Result<Self> {
        Ok(Toggle {
            id,
            text: ButtonWidget::new(config, id).with_content(block_config.text),
            command_on: block_config.command_on,
            command_off: block_config.command_off,
            command_state: block_config.command_state,
            icon_on: block_config.icon_on,
            icon_off: block_config.icon_off,
            toggled: false,
            update_interval: block_config.interval,
        })
    }
}

impl Block for Toggle {
    fn update(&mut self) -> Result<Option<Update>> {
        let output = Command::new(env::var("SHELL").unwrap_or_else(|_| "sh".to_owned()))
            .args(&["-c", &self.command_state])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
            .unwrap_or_else(|e| e.to_string());

        self.text.set_icon(match output.trim_start() {
            "" => {
                self.toggled = false;
                self.icon_off.as_str()
            }
            _ => {
                self.toggled = true;
                self.icon_on.as_str()
            }
        });

        self.text.set_state(State::Idle);

        Ok(self.update_interval.map(|d| d.into()))
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.text]
    }

    fn click(&mut self, e: &I3BarEvent) -> Result<()> {
        if e.matches_id(self.id) {
            let cmd = if self.toggled {
                &self.command_off
            } else {
                &self.command_on
            };

            let output = Command::new(env::var("SHELL").unwrap_or_else(|_| "sh".to_owned()))
                .args(&["-c", cmd])
                .output()
                .block_error("toggle", "failed to run toggle command")?;

            if output.status.success() {
                self.text.set_state(State::Idle);
                self.toggled = !self.toggled;
                self.text.set_icon(if self.toggled {
                    self.icon_on.as_str()
                } else {
                    self.icon_off.as_str()
                })
            } else {
                self.text.set_state(State::Critical);
            };
        }

        Ok(())
    }

    fn id(&self) -> usize {
        self.id
    }
}

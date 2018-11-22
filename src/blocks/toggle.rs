use std::env;
use std::time::Duration;
use std::process::Command;
use chan::Sender;
use scheduler::Task;

use block::{Block, ConfigBlock};
use config::Config;
use de::deserialize_opt_duration;
use errors::*;
use widgets::button::ButtonWidget;
use widget::I3BarWidget;
use input::I3BarEvent;

use uuid::Uuid;

pub struct Toggle {
    text: ButtonWidget,
    command_on: String,
    command_off: String,
    command_state: String,
    icon_on: String,
    icon_off: String,
    update_interval: Option<Duration>,
    toggled: bool,
    id: String,
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
}

impl ToggleConfig {
    fn default_icon_on() -> String {
        "toggle_on".to_owned()
    }

    fn default_icon_off() -> String {
        "toggle_off".to_owned()
    }
}

impl ConfigBlock for Toggle {
    type Config = ToggleConfig;

    fn new(block_config: Self::Config, config: Config, _tx_update_request: Sender<Task>) -> Result<Self> {
        let id = Uuid::new_v4().simple().to_string();
        Ok(Toggle {
            text: ButtonWidget::new(config, &id).with_content(block_config.text),
            command_on: block_config.command_on,
            command_off: block_config.command_off,
            command_state: block_config.command_state,
            icon_on: block_config.icon_on,
            icon_off: block_config.icon_off,
            id,
            toggled: false,
            update_interval: block_config.interval,
        })
    }
}

impl Block for Toggle {
    fn update(&mut self) -> Result<Option<Duration>> {
        let output = Command::new(env::var("SHELL").unwrap_or("sh".to_owned()))
            .args(&["-c", &self.command_state])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
            .unwrap_or_else(|e| e.description().to_owned());

        self.text.set_icon(match output.trim_left() {
            "" => {
                self.toggled = false;
                self.icon_off.as_str()
            }
            _ => {
                self.toggled = true;
                self.icon_on.as_str()
            }
        });

        Ok(self.update_interval)
    }

    fn view(&self) -> Vec<&I3BarWidget> {
        vec![&self.text]
    }

    fn click(&mut self, e: &I3BarEvent) -> Result<()> {
        if let Some(ref name) = e.name {
            if name.as_str() == self.id {
                let cmd = if self.toggled {
                    self.toggled = false;
                    self.text.set_icon(self.icon_off.as_str());
                    &self.command_off
                } else {
                    self.toggled = true;
                    self.text.set_icon(self.icon_on.as_str());
                    &self.command_on
                };

                Command::new(env::var("SHELL").unwrap_or("sh".to_owned()))
                    .args(&["-c", cmd])
                    .output()
                    .block_error("toggle", "failed to run toggle command")?;
            }
        }

        Ok(())
    }

    fn id(&self) -> &str {
        &self.id
    }
}

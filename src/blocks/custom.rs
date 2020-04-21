use crossbeam_channel::Sender;
use serde_derive::Deserialize;
use serde_json;
use std::env;
use std::iter::{Cycle, Peekable};
use std::process::Command;
use std::time::{Duration, Instant};
use std::vec;

use crate::blocks::{Block, ConfigBlock};
use crate::config::Config;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::input::I3BarEvent;
use crate::scheduler::Task;
use crate::subprocess::spawn_child_async;
use crate::widget::{I3BarWidget, State};
use crate::widgets::button::ButtonWidget;

use uuid::Uuid;

pub struct Custom {
    id: String,
    update_interval: Duration,
    output: ButtonWidget,
    command: Option<String>,
    on_click: Option<String>,
    cycle: Option<Peekable<Cycle<vec::IntoIter<String>>>>,
    tx_update_request: Sender<Task>,
    pub json: bool,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct CustomConfig {
    /// Update interval in seconds
    #[serde(
        default = "CustomConfig::default_interval",
        deserialize_with = "deserialize_duration"
    )]
    pub interval: Duration,

    /// Shell Command to execute & display
    pub command: Option<String>,

    /// Command to execute when the button is clicked
    pub on_click: Option<String>,

    /// Commands to execute and change when the button is clicked
    pub cycle: Option<Vec<String>>,

    /// Parse command output if it contains valid bar JSON
    #[serde(default = "CustomConfig::default_json")]
    pub json: bool,
}

impl CustomConfig {
    fn default_interval() -> Duration {
        Duration::from_secs(10)
    }

    fn default_json() -> bool {
        false
    }
}

impl ConfigBlock for Custom {
    type Config = CustomConfig;

    fn new(block_config: Self::Config, config: Config, tx: Sender<Task>) -> Result<Self> {
        let mut custom = Custom {
            id: Uuid::new_v4().to_simple().to_string(),
            update_interval: block_config.interval,
            output: ButtonWidget::new(config.clone(), ""),
            command: None,
            on_click: None,
            cycle: None,
            tx_update_request: tx,
            json: block_config.json,
        };
        custom.output = ButtonWidget::new(config, &custom.id);

        if let Some(on_click) = block_config.on_click {
            custom.on_click = Some(on_click.to_string())
        };

        if let Some(cycle) = block_config.cycle {
            custom.cycle = Some(cycle.into_iter().cycle().peekable());
            return Ok(custom);
        };

        if let Some(command) = block_config.command {
            custom.command = Some(command.to_string())
        };

        Ok(custom)
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
    fn update(&mut self) -> Result<Option<Duration>> {
        let command_str = self
            .cycle
            .as_mut()
            .map(|c| c.peek().cloned().unwrap_or_else(|| "".to_owned()))
            .or_else(|| self.command.clone())
            .unwrap_or_else(|| "".to_owned());

        let raw_output = Command::new(env::var("SHELL").unwrap_or("sh".to_owned()))
            .args(&["-c", &command_str])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
            .unwrap_or_else(|e| e.to_string());

        if self.json {
            let output: Output =
                serde_json::from_str(&*raw_output).block_error("custom", "invalid JSON")?;
            self.output.set_icon(&output.icon);
            self.output.set_state(output.state);
            self.output.set_text(output.text);
        } else {
            self.output.set_text(raw_output);
        }

        Ok(Some(self.update_interval))
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.output]
    }

    fn click(&mut self, event: &I3BarEvent) -> Result<()> {
        if let Some(ref name) = event.name {
            if name != &self.id {
                return Ok(());
            }
        } else {
            return Ok(());
        }

        let mut update = false;

        if let Some(ref on_click) = self.on_click {
            spawn_child_async("sh", &["-c", on_click]).ok();
            update = true;
        }

        if let Some(ref mut cycle) = self.cycle {
            cycle.next();
            update = true;
        }

        if update {
            self.tx_update_request.send(Task {
                id: self.id.clone(),
                update_time: Instant::now(),
            })?;
        }

        Ok(())
    }

    fn id(&self) -> &str {
        &self.id
    }
}

use crossbeam_channel::Sender;
use serde_derive::Deserialize;
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
use crate::widget::I3BarWidget;
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
}

impl CustomConfig {
    fn default_interval() -> Duration {
        Duration::from_secs(10)
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

impl Block for Custom {
    fn update(&mut self) -> Result<Option<Duration>> {
        let command_str = self
            .cycle
            .as_mut()
            .map(|c| c.peek().cloned().unwrap_or_else(|| "".to_owned()))
            .or_else(|| self.command.clone())
            .unwrap_or_else(|| "".to_owned());

        let output = Command::new(env::var("SHELL").unwrap_or("sh".to_owned()))
            .args(&["-c", &command_str])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
            .unwrap_or_else(|e| e.to_string());

        self.output.set_text(output);

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
            Command::new(env::var("SHELL").unwrap_or_else(|_| "sh".to_owned()))
                .args(&["-c", on_click])
                .output()
                .ok();
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

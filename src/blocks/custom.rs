use std::time::{Duration, Instant};
use std::process::Command;
use std::iter::{Cycle, Peekable};
use std::vec;
use std::env;
use crossbeam_channel::Sender;

use crate::block::{Block, ConfigBlock};
use crate::config::Config;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::widgets::button::ButtonWidget;
use crate::widget::{I3BarWidget, State};
use crate::input::I3BarEvent;
use crate::scheduler::Task;

use uuid::Uuid;

pub struct Custom {
    id: String,
    update_interval: Duration,
    output: ButtonWidget,
    command: Option<String>,
    on_click: Option<String>,
    cycle: Option<Peekable<Cycle<vec::IntoIter<String>>>>,
    tx_update_request: Sender<Task>,
    info_exit_codes     : Vec<i32>,
    good_exit_codes     : Vec<i32>,
    warning_exit_codes  : Vec<i32>,
    critical_exit_codes : Vec<i32>,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct CustomConfig {
    /// Update interval in seconds
    #[serde(default = "CustomConfig::default_interval", deserialize_with = "deserialize_duration")]
    pub interval: Duration,

    /// Shell Command to execute & display
    pub command: Option<String>,

    /// Command to execute when the button is clicked
    pub on_click: Option<String>,

    /// Commands to execute and change when the button is clicked
    pub cycle: Option<Vec<String>>,

    /// Exit codes to change the status to info
    pub info_exit_codes     : Option<Vec<i32>>,

    /// Exit codes to change the status to good
    pub good_exit_codes     : Option<Vec<i32>>,
    
    /// Exit codes to change the status to warning
    pub warning_exit_codes  : Option<Vec<i32>>,

    /// Exit codes to change the status to critical
    pub critical_exit_codes : Option<Vec<i32>>,
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
            id: Uuid::new_v4().simple().to_string(),
            update_interval: block_config.interval,
            output: ButtonWidget::new(config.clone(), ""),
            command: None,
            on_click: None,
            cycle: None,
            tx_update_request: tx,
            info_exit_codes:     block_config.info_exit_codes.unwrap_or(vec![]),
            good_exit_codes:     block_config.good_exit_codes.unwrap_or(vec![]),
            warning_exit_codes:  block_config.warning_exit_codes.unwrap_or(vec![]),
            critical_exit_codes: block_config.critical_exit_codes.unwrap_or(vec![]),
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
        let command_str = self.cycle
            .as_mut()
            .map(|c| c.peek().cloned().unwrap_or_else(|| "".to_owned()))
            .or_else(|| self.command.clone())
            .unwrap_or_else(|| "".to_owned());

        let (statuscode, output) = Command::new(env::var("SHELL").unwrap_or("sh".to_owned()))
            .args(&["-c", &command_str])
            .output()
            .map(|o| (o.status.code().unwrap_or(254) , String::from_utf8_lossy(&o.stdout).trim().to_owned()))
            .unwrap_or_else(|e| (255, e.description().to_owned()));

        self.output.set_text(output);
        self.output.set_state( 
                if self.critical_exit_codes.contains(&statuscode) {
                    State::Critical
                } else if self.warning_exit_codes.contains(&statuscode) {
                    State::Warning
                } else if self.good_exit_codes.contains(&statuscode) {
                    State::Good
                } else if self.info_exit_codes.contains(&statuscode) {
                    State::Info
                } else {
                    State::Idle
                }
        );

        Ok(Some(self.update_interval))
    }

    fn view(&self) -> Vec<&I3BarWidget> {
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
            Command::new(env::var("SHELL").unwrap_or_else(|_|"sh".to_owned()))
                    .args(&["-c", on_click]).output().ok();
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

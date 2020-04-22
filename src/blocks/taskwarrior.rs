use std::process::Command;
use std::time::Duration;

use crossbeam_channel::Sender;
use serde_derive::Deserialize;
use uuid::Uuid;

use crate::blocks::{Block, ConfigBlock};
use crate::config::Config;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::input::I3BarEvent;
use crate::scheduler::Task;
use crate::widget::{I3BarWidget, State};
use crate::widgets::button::ButtonWidget;

pub struct Taskwarrior {
    output: ButtonWidget,
    id: String,
    update_interval: Duration,
    warning_threshold: u32,
    critical_threshold: u32,

    //useful, but optional
    #[allow(dead_code)]
    config: Config,
    #[allow(dead_code)]
    tx_update_request: Sender<Task>,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct TaskwarriorConfig {
    /// Update interval in seconds
    #[serde(
        default = "TaskwarriorConfig::default_interval",
        deserialize_with = "deserialize_duration"
    )]
    pub interval: Duration,

    // Threshold from which on the block is marked with a warning indicator
    #[serde(default = "TaskwarriorConfig::default_threshold_warning")]
    pub warning_threshold: u32,

    // Threshold from which on the block is marked with a critical indicator
    #[serde(default = "TaskwarriorConfig::default_threshold_critical")]
    pub critical_threshold: u32,
}

impl TaskwarriorConfig {
    fn default_interval() -> Duration {
        Duration::from_secs(60)
    }

    fn default_threshold_warning() -> u32 {
        10
    }

    fn default_threshold_critical() -> u32 {
        20
    }
}

impl ConfigBlock for Taskwarrior {
    type Config = TaskwarriorConfig;

    fn new(
        block_config: Self::Config,
        config: Config,
        tx_update_request: Sender<Task>,
    ) -> Result<Self> {
        Ok(Taskwarrior {
            id: Uuid::new_v4().to_simple().to_string(),
            update_interval: block_config.interval,
            warning_threshold: block_config.warning_threshold,
            critical_threshold: block_config.critical_threshold,
            output: ButtonWidget::new(config.clone(), "taskwarrior")
                .with_icon("tasks")
                .with_text("-"),
            tx_update_request,
            config,
        })
    }
}

fn has_taskwarrior() -> Result<bool> {
    Ok(String::from_utf8(
        Command::new("sh")
            .args(&["-c", "type -P task"])
            .output()
            .block_error(
                "taskwarrior",
                "failed to start command to check for taskwarrior",
            )?
            .stdout,
    )
    .block_error("taskwarrior", "failed to check for taskwarrior")?
    .trim()
        != "")
}

fn get_number_of_pending_tasks() -> Result<u32> {
    String::from_utf8(
        Command::new("sh")
            .args(&["-c", "task -COMPLETED count"])
            .output()
            .block_error(
                "taskwarrior",
                "failed to run taskwarrior for getting the number of pending tasks",
            )?
            .stdout,
    )
    .block_error(
        "taskwarrior",
        "failed to get the number of pending tasks from taskwarrior",
    )?
    .trim()
    .parse::<u32>()
    .block_error("taskwarrior", "could not parse the result of taskwarrior")
}

impl Block for Taskwarrior {
    fn update(&mut self) -> Result<Option<Duration>> {
        // if the taskwarrior binary is not installed, set the output to a questionmark
        if !has_taskwarrior()? {
            self.output.set_text("?")
        } else {
            let number_of_pending_tasks = get_number_of_pending_tasks()?;
            self.output.set_text(format!("{}", number_of_pending_tasks));
            if number_of_pending_tasks >= self.critical_threshold {
                self.output.set_state(State::Critical);
            } else if number_of_pending_tasks >= self.warning_threshold {
                self.output.set_state(State::Warning);
            } else {
                self.output.set_state(State::Idle);
            }
        }

        // continue updating the block in the configured interval
        Ok(Some(self.update_interval))
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.output]
    }

    fn click(&mut self, _: &I3BarEvent) -> Result<()> {
        Ok(())
    }

    fn id(&self) -> &str {
        &self.id
    }
}

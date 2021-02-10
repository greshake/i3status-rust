use std::collections::BTreeMap;
use std::process::Command;
use std::time::Duration;

use crossbeam_channel::Sender;
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::Config;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::input::{I3BarEvent, MouseButton};
use crate::scheduler::Task;
use crate::util::FormatTemplate;
use crate::widget::{I3BarWidget, State};
use crate::widgets::button::ButtonWidget;

pub struct Taskwarrior {
    id: usize,
    output: ButtonWidget,
    update_interval: Duration,
    warning_threshold: u32,
    critical_threshold: u32,
    filters: Vec<Filter>,
    filter_index: usize,
    format: FormatTemplate,
    format_singular: FormatTemplate,
    format_everything_done: FormatTemplate,

    //useful, but optional
    #[allow(dead_code)]
    config: Config,
    #[allow(dead_code)]
    tx_update_request: Sender<Task>,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct Filter {
    pub name: String,
    pub filter: String,
}

impl Filter {
    pub fn new(name: String, filter: String) -> Self {
        Filter { name, filter }
    }

    pub fn legacy(name: String, tags: &[String]) -> Self {
        let tags = tags
            .iter()
            .map(|element| format!("+{}", element))
            .collect::<Vec<String>>()
            .join(" ");
        let filter = format!("-COMPLETED -DELETED {}", tags);
        Self::new(name, filter)
    }
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

    /// Threshold from which on the block is marked with a warning indicator
    #[serde(default = "TaskwarriorConfig::default_threshold_warning")]
    pub warning_threshold: u32,

    /// Threshold from which on the block is marked with a critical indicator
    #[serde(default = "TaskwarriorConfig::default_threshold_critical")]
    pub critical_threshold: u32,

    /// A list of tags a task has to have before it's used for counting pending tasks
    /// (DEPRECATED) use filters instead
    #[serde(default = "TaskwarriorConfig::default_filter_tags")]
    pub filter_tags: Vec<String>,

    /// A list of named filter criteria which must be fulfilled to be counted towards
    /// the total, when that filter is active.
    #[serde(default = "TaskwarriorConfig::default_filters")]
    pub filters: Vec<Filter>,

    /// Format override
    #[serde(default = "TaskwarriorConfig::default_format")]
    pub format: String,

    /// Format override if the count is one
    #[serde(default = "TaskwarriorConfig::default_format")]
    pub format_singular: String,

    /// Format override if the count is zero
    #[serde(default = "TaskwarriorConfig::default_format")]
    pub format_everything_done: String,

    #[serde(default = "TaskwarriorConfig::default_color_overrides")]
    pub color_overrides: Option<BTreeMap<String, String>>,
}

impl TaskwarriorConfig {
    fn default_interval() -> Duration {
        Duration::from_secs(600)
    }

    fn default_threshold_warning() -> u32 {
        10
    }

    fn default_threshold_critical() -> u32 {
        20
    }

    fn default_filter_tags() -> Vec<String> {
        vec![]
    }

    fn default_filters() -> Vec<Filter> {
        vec![Filter::new(
            "pending".to_string(),
            "-COMPLETED -DELETED".to_string(),
        )]
    }

    fn default_format() -> String {
        "{count}".to_owned()
    }

    fn default_color_overrides() -> Option<BTreeMap<String, String>> {
        None
    }
}

impl ConfigBlock for Taskwarrior {
    type Config = TaskwarriorConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        config: Config,
        tx_update_request: Sender<Task>,
    ) -> Result<Self> {
        let output = ButtonWidget::new(config.clone(), id)
            .with_icon("tasks")
            .with_text("-");
        // If the deprecated `filter_tags` option has been set,
        // convert it to the new `filter` format.
        let filters = if !block_config.filter_tags.is_empty() {
            vec![
                Filter::legacy("filtered".to_string(), &block_config.filter_tags),
                Filter::legacy("all".to_string(), &[]),
            ]
        } else {
            block_config.filters
        };

        Ok(Taskwarrior {
            id,
            update_interval: block_config.interval,
            warning_threshold: block_config.warning_threshold,
            critical_threshold: block_config.critical_threshold,
            format: FormatTemplate::from_string(&block_config.format).block_error(
                "taskwarrior",
                "Invalid format specified for taskwarrior::format",
            )?,
            format_singular: FormatTemplate::from_string(&block_config.format_singular)
                .block_error(
                    "taskwarrior",
                    "Invalid format specified for taskwarrior::format_singular",
                )?,
            format_everything_done: FormatTemplate::from_string(
                &block_config.format_everything_done,
            )
            .block_error(
                "taskwarrior",
                "Invalid format specified for taskwarrior::format_everything_done",
            )?,
            tx_update_request,
            filter_index: 0,
            config,
            filters,
            output,
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

fn get_number_of_tasks(filter: &str) -> Result<u32> {
    String::from_utf8(
        Command::new("sh")
            .args(&["-c", &format!("task rc.gc=off {} count", filter)])
            .output()
            .block_error(
                "taskwarrior",
                "failed to run taskwarrior for getting the number of tasks",
            )?
            .stdout,
    )
    .block_error(
        "taskwarrior",
        "failed to get the number of tasks from taskwarrior",
    )?
    .trim()
    .parse::<u32>()
    .block_error("taskwarrior", "could not parse the result of taskwarrior")
}

impl Block for Taskwarrior {
    fn update(&mut self) -> Result<Option<Update>> {
        if !has_taskwarrior()? {
            self.output.set_text("?")
        } else {
            let filter = self.filters.get(self.filter_index).block_error(
                "taskwarrior",
                &format!("Filter at index {} does not exist", self.filter_index),
            )?;
            let number_of_tasks = get_number_of_tasks(&filter.filter)?;
            let values = map!(
                "{count}" => number_of_tasks.to_string(),
                "{filter_name}" => filter.name.clone()
            );
            self.output.set_text(match number_of_tasks {
                0 => self.format_everything_done.render_static_str(&values)?,
                1 => self.format_singular.render_static_str(&values)?,
                _ => self.format.render_static_str(&values)?,
            });
            if number_of_tasks >= self.critical_threshold {
                self.output.set_state(State::Critical);
            } else if number_of_tasks >= self.warning_threshold {
                self.output.set_state(State::Warning);
            } else {
                self.output.set_state(State::Idle);
            }
        }

        // continue updating the block in the configured interval
        Ok(Some(self.update_interval.into()))
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.output]
    }

    fn click(&mut self, event: &I3BarEvent) -> Result<()> {
        if event.matches_id(self.id) {
            match event.button {
                MouseButton::Left => {
                    self.update()?;
                }
                MouseButton::Right => {
                    // Increment the filter_index, rotating at the end
                    self.filter_index = (self.filter_index + 1) % self.filters.len();
                    self.update()?;
                }
                _ => {}
            }
        }

        Ok(())
    }

    fn id(&self) -> usize {
        self.id
    }
}

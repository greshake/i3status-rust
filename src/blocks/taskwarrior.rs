use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::SharedConfig;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::formatting::value::Value;
use crate::formatting::FormatTemplate;
use crate::protocol::i3bar_event::{I3BarEvent, MouseButton};
use crate::scheduler::Task;
use crate::widgets::text::TextWidget;
use crate::widgets::{I3BarWidget, State};
use inotify::{EventMask, Inotify, WatchMask};

pub struct Taskwarrior {
    id: usize,
    output: TextWidget,
    update_interval: Duration,
    warning_threshold: u32,
    critical_threshold: u32,
    filters: Vec<Filter>,
    filter_index: usize,
    format: FormatTemplate,
    format_singular: FormatTemplate,
    format_everything_done: FormatTemplate,
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

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct TaskwarriorConfig {
    /// Update interval in seconds
    #[serde(deserialize_with = "deserialize_duration")]
    pub interval: Duration,

    /// Threshold from which on the block is marked with a warning indicator
    pub warning_threshold: u32,

    /// Threshold from which on the block is marked with a critical indicator
    pub critical_threshold: u32,

    /// A list of tags a task has to have before it's used for counting pending tasks
    /// (DEPRECATED) use filters instead
    pub filter_tags: Vec<String>,

    /// A list of named filter criteria which must be fulfilled to be counted towards
    /// the total, when that filter is active.
    pub filters: Vec<Filter>,

    /// Format override
    pub format: FormatTemplate,

    /// Format override if the count is one
    pub format_singular: FormatTemplate,

    /// Format override if the count is zero
    pub format_everything_done: FormatTemplate,

    /// Data directory. Defaults to ~/.task but it's configurable in taskwarrior
    /// (data.location in .taskrc) so make it configurable here, too
    pub data_location: String,
}

impl Default for TaskwarriorConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(600),
            warning_threshold: 10,
            critical_threshold: 20,
            filter_tags: vec![],
            filters: vec![Filter::new(
                "pending".to_string(),
                "-COMPLETED -DELETED".to_string(),
            )],
            format: FormatTemplate::default(),
            format_singular: FormatTemplate::default(),
            format_everything_done: FormatTemplate::default(),
            data_location: "~/.task".to_string(),
        }
    }
}

impl ConfigBlock for Taskwarrior {
    type Config = TaskwarriorConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        shared_config: SharedConfig,
        tx_update_request: Sender<Task>,
    ) -> Result<Self> {
        let output = TextWidget::new(id, 0, shared_config)
            .with_icon("tasks")?
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

        let data_location = block_config.data_location.clone();
        let data_location = shellexpand::full(data_location.as_str())
            .map_err(|e| {
                ConfigurationError(
                    "custom".to_string(),
                    format!("Failed to expand data location {}: {}", data_location, e),
                )
            })?
            .to_string();

        // Spin up a thread to watch for changes to the task directory (~/.task)
        // and schedule an update if needed.
        thread::Builder::new()
            .name("taskwarrior".into())
            .spawn(move || {
                let mut notify = Inotify::init().expect("Failed to start inotify");
                notify
                    .add_watch(data_location, WatchMask::MODIFY)
                    .expect("Failed to watch task directory");

                let mut buffer = [0; 1024];
                loop {
                    let mut events = notify
                        .read_events_blocking(&mut buffer)
                        .expect("Error while reading inotify events");

                    if events.any(|event| event.mask.contains(EventMask::MODIFY)) {
                        tx_update_request
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

        Ok(Taskwarrior {
            id,
            update_interval: block_config.interval,
            warning_threshold: block_config.warning_threshold,
            critical_threshold: block_config.critical_threshold,
            format: block_config.format.with_default("{count}")?,
            format_singular: block_config.format_singular.with_default("{count}")?,
            format_everything_done: block_config
                .format_everything_done
                .with_default("{count}")?,
            filter_index: 0,
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
            self.output.set_text("?".to_string())
        } else {
            let filter = self.filters.get(self.filter_index).block_error(
                "taskwarrior",
                &format!("Filter at index {} does not exist", self.filter_index),
            )?;
            let number_of_tasks = get_number_of_tasks(&filter.filter)?;
            let values = map!(
                "count" => Value::from_integer(number_of_tasks as i64),
                "filter_name" => Value::from_string(filter.name.clone()),
            );
            self.output.set_texts(match number_of_tasks {
                0 => self.format_everything_done.render(&values)?,
                1 => self.format_singular.render(&values)?,
                _ => self.format.render(&values)?,
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

        Ok(())
    }

    fn id(&self) -> usize {
        self.id
    }
}

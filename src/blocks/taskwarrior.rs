//! The number of tasks from the taskwarrior list
//!
//! Clicking on the block updates the number of tasks immediately. Clicking the right mouse button on the icon cycles the view of the block through the user's filters.
//!
//! # Configuration
//!
//! Key | Values | Required | Default
//! ----|--------|----------|--------
//! `interval` | Update interval in seconds | No | `600` (10min)
//! `warning_threshold` | The threshold of pending (or started) tasks when the block turns into a warning state | No | `10`
//! `critical_threshold` | The threshold of pending (or started) tasks when the block turns into a critical state | No | `20`
//! `hide_when_zero` | Whethere to hide the block when the number of tasks is zero | No | `false`
//! `filters` | A list of tables with the keys `name` and `filter`. `filter` specifies the criteria that must be met for a task to be counted towards this filter. | No | ```[{name = "pending", filter = "-COMPLETED -DELETED"}]```
//! `format` | A string to customise the output of this block. See below for available placeholders. | No | `"$done|$count.eng(1)"`
//! `data_location`| Directory in which taskwarrior stores its data files. | No | "~/.task"`
//!
//! Placeholder   | Value                                       | Type   | Unit
//! --------------|---------------------------------------------|--------|-----
//! `count`       | The number of tasks matching current filter | Number | -
//! `filter_name` | The name of current filter                  | Text   | -
//! `done`        | Present only if `count` is zero             | Flag   | -
//! `single`      | Present only if `count` is one              | Flag   | -
//!
//! # Example
//!
//! In this example, block will display "All done" if `count` is zero, "One task" if `count` is one
//! and "Tasks: N" if there are more than one task.
//!
//! ```toml
//! [[block]]
//! block = "taskwarrior"
//! interval = 60
//! format = "$done{All done}|$single{One task}|Tasks: $count.eng(1)"
//! warning_threshold = 10
//! critical_threshold = 20
//! [[block.filters]]
//! name = "today"
//! filter = "+PENDING +OVERDUE or +DUETODAY"
//! [[block.filters]]
//! name = "some-project"
//! filter = "project:some-project +PENDING"
//! ```
//!
//! # Icons Used
//! - `tasks`

use super::prelude::*;
use inotify::{Inotify, WatchMask};
use tokio::process::Command;

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields, default)]
struct TaskwarriorConfig {
    interval: Seconds,
    warning_threshold: u32,
    critical_threshold: u32,
    hide_when_zero: bool,
    filters: Vec<Filter>,
    format: FormatConfig,
    data_location: ShellString,
}

impl Default for TaskwarriorConfig {
    fn default() -> Self {
        Self {
            interval: Seconds::new(600),
            warning_threshold: 10,
            critical_threshold: 20,
            hide_when_zero: false,
            filters: vec![Filter {
                name: "pending".into(),
                filter: "-COMPLETED -DELETED".into(),
            }],
            format: FormatConfig::default(),
            data_location: ShellString::new("~/.task"),
        }
    }
}

pub async fn run(config: toml::Value, mut api: CommonApi) -> Result<()> {
    let mut events = api.get_events().await?;
    let config = TaskwarriorConfig::deserialize(config).config_error()?;
    api.set_format(config.format.with_default("$done|$count.eng(1)")?);
    api.set_icon("tasks")?;

    let mut filters = config.filters.iter().cycle();
    let mut filter = filters.next().error("failed to get next filter")?;

    let mut notify = Inotify::init().error("Failed to start inotify")?;
    let mut buffer = [0; 1024];
    notify
        .add_watch(&*config.data_location.expand()?, WatchMask::MODIFY)
        .error("Failed to watch data location")?;
    let mut updates = notify
        .event_stream(&mut buffer)
        .error("Failed to create event stream")?;

    loop {
        let number_of_tasks = get_number_of_tasks(&filter.filter).await?;

        if number_of_tasks != 0 || !config.hide_when_zero {
            let mut values = map!(
                "count" => Value::number(number_of_tasks),
                "filter_name" => Value::text(filter.name.clone()),
            );
            if number_of_tasks == 0 {
                values.insert("done".into(), Value::Flag);
            } else if number_of_tasks == 1 {
                values.insert("single".into(), Value::Flag);
            }
            api.set_values(values);

            api.set_state(if number_of_tasks >= config.critical_threshold {
                State::Critical
            } else if number_of_tasks >= config.warning_threshold {
                State::Warning
            } else {
                State::Idle
            });

            api.show();
        } else {
            api.hide();
        }

        api.flush().await?;

        tokio::select! {
            _ = sleep(config.interval.0) =>(),
            _ = updates.next() => (),
            Some(BlockEvent::Click(click)) = events.recv() => {
                if click.button == MouseButton::Right {
                    filter = filters.next().error("failed to get next filter")?;
                }
            }
        }
    }
}

async fn get_number_of_tasks(filter: &str) -> Result<u32> {
    let output = Command::new("task")
        .args(&["rc.gc=off", filter, "count"])
        .output()
        .await
        .error("failed to run taskwarrior for getting the number of tasks")?
        .stdout;
    std::str::from_utf8(&output)
        .error("failed to get the number of tasks from taskwarrior (invalid UTF-8)")?
        .trim()
        .parse::<u32>()
        .error("could not parse the result of taskwarrior")
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
struct Filter {
    pub name: String,
    pub filter: String,
}

//! The number of tasks from the taskwarrior list
//!
//! Clicking the right mouse button on the icon cycles the view of the block through the user's filters.
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `interval` | Update interval in seconds | `600` (10min)
//! `warning_threshold` | The threshold of pending (or started) tasks when the block turns into a warning state | `10`
//! `critical_threshold` | The threshold of pending (or started) tasks when the block turns into a critical state | `20`
//! `filters` | A list of tables with the keys `name` and `filter`. `filter` specifies the criteria that must be met for a task to be counted towards this filter. | ```[{name = "pending", filter = "-COMPLETED -DELETED"}]```
//! `format` | A string to customise the output of this block. See below for available placeholders. | `" $icon $count.eng(w:1) "`
//! `format_singular` | Same as `format` but for when exactly one task is pending. | `" $icon $count.eng(w:1) "`
//! `format_everything_done` | Same as `format` but for when all tasks are completed. | `" $icon $count.eng(w:1) "`
//! `data_location`| Directory in which taskwarrior stores its data files. Supports path expansions e.g. `~`. | `"~/.task"`
//!
//! Placeholder   | Value                                       | Type   | Unit
//! --------------|---------------------------------------------|--------|-----
//! `icon`        | A static icon                               | Icon   | -
//! `count`       | The number of tasks matching current filter | Number | -
//! `filter_name` | The name of current filter                  | Text   | -
//!
//! Action        | Default button
//! --------------|---------------
//! `next_filter` | Right
//!
//! # Example
//!
//! In this example, block will be hidden if `count` is zero.
//!
//! ```toml
//! [[block]]
//! block = "taskwarrior"
//! interval = 60
//! format = " $icon count.eng(w:1) tasks "
//! format_singular = " $icon 1 task "
//! format_everything_done = ""
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
pub struct Config {
    pub interval: Seconds,
    pub warning_threshold: u32,
    pub critical_threshold: u32,
    pub filters: Vec<Filter>,
    pub format: FormatConfig,
    pub format_singular: FormatConfig,
    pub format_everything_done: FormatConfig,
    pub data_location: ShellString,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            interval: Seconds::new(600),
            warning_threshold: 10,
            critical_threshold: 20,
            filters: vec![Filter {
                name: "pending".into(),
                filter: "-COMPLETED -DELETED".into(),
            }],
            format: default(),
            format_singular: default(),
            format_everything_done: default(),
            data_location: ShellString::new("~/.task"),
        }
    }
}

pub async fn run(config: &Config, api: &CommonApi) -> Result<()> {
    let mut actions = api.get_actions()?;
    api.set_default_actions(&[(MouseButton::Right, None, "next_filter")])?;

    let format = config.format.with_default(" $icon $count.eng(w:1) ")?;
    let format_singular = config
        .format_singular
        .with_default(" $icon $count.eng(w:1) ")?;
    let format_everything_done = config
        .format_everything_done
        .with_default(" $icon $count.eng(w:1) ")?;

    let mut filters = config.filters.iter().cycle();
    let mut filter = filters.next().error("`filters` is empty")?;

    let notify = Inotify::init().error("Failed to start inotify")?;
    notify
        .watches()
        .add(&*config.data_location.expand()?, WatchMask::MODIFY)
        .error("Failed to watch data location")?;
    let mut updates = notify
        .into_event_stream([0; 1024])
        .error("Failed to create event stream")?;

    loop {
        let number_of_tasks = get_number_of_tasks(&filter.filter).await?;

        let mut widget = Widget::new();

        widget.set_format(match number_of_tasks {
            0 => format_everything_done.clone(),
            1 => format_singular.clone(),
            _ => format.clone(),
        });

        widget.set_values(map! {
            "icon" => Value::icon("tasks"),
            "count" => Value::number(number_of_tasks),
            "filter_name" => Value::text(filter.name.clone()),
        });

        widget.state = match number_of_tasks {
            x if x >= config.critical_threshold => State::Critical,
            x if x >= config.warning_threshold => State::Warning,
            _ => State::Idle,
        };

        api.set_widget(widget)?;

        select! {
            _ = sleep(config.interval.0) =>(),
            _ = updates.next() => (),
            _ = api.wait_for_update_request() => (),
            Some(action) = actions.recv() => match action.as_ref() {
                "next_filter" => {
                    filter = filters.next().unwrap();
                }
                _ => (),
            }
        }
    }
}

async fn get_number_of_tasks(filter: &str) -> Result<u32> {
    let output = Command::new("task")
        .args(["rc.gc=off", filter, "count"])
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
pub struct Filter {
    pub name: String,
    pub filter: String,
}

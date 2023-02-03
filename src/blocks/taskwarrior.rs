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
//! `hide_when_zero` | Whethere to hide the block when the number of tasks is zero | `false`
//! `filters` | A list of tables with the keys `name` and `filter`. `filter` specifies the criteria that must be met for a task to be counted towards this filter. | ```[{name = "pending", filter = "-COMPLETED -DELETED"}]```
//! `format` | A string to customise the output of this block. See below for available placeholders. | `" $icon $done\|$count.eng(w:1) "`
//! `data_location`| Directory in which taskwarrior stores its data files. Supports path expansions e.g. `~`. | `"~/.task"`
//!
//! Placeholder   | Value                                       | Type   | Unit
//! --------------|---------------------------------------------|--------|-----
//! `icon`        | A static icon                               | Icon   | -
//! `count`       | The number of tasks matching current filter | Number | -
//! `filter_name` | The name of current filter                  | Text   | -
//! `done`        | Present only if `count` is zero             | Flag   | -
//! `single`      | Present only if `count` is one              | Flag   | -
//!
//! Action        | Default button
//! --------------|---------------
//! `next_filter` | Right
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
//! format = " $icon $done{All done}|$single{One task}|Tasks: $count.eng(w:1) "
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
#[serde(default)]
pub struct Config {
    interval: Seconds,
    warning_threshold: u32,
    critical_threshold: u32,
    hide_when_zero: bool,
    filters: Vec<Filter>,
    format: FormatConfig,
    data_location: ShellString,
}

impl Default for Config {
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
            format: default(),
            data_location: ShellString::new("~/.task"),
        }
    }
}

pub async fn run(config: Config, mut api: CommonApi) -> Result<()> {
    api.set_default_actions(&[(MouseButton::Right, None, "next_filter")])
        .await?;

    let mut widget = Widget::new().with_format(
        config
            .format
            .with_default(" $icon $done|$count.eng(w:1) ")?,
    );

    let mut filters = config.filters.iter().cycle();
    let mut filter = filters.next().error("`filters` is empty")?;

    let mut notify = Inotify::init().error("Failed to start inotify")?;
    notify
        .add_watch(&*config.data_location.expand()?, WatchMask::MODIFY)
        .error("Failed to watch data location")?;
    let mut updates = notify
        .event_stream([0; 1024])
        .error("Failed to create event stream")?;

    loop {
        let number_of_tasks = get_number_of_tasks(&filter.filter).await?;

        if number_of_tasks != 0 || !config.hide_when_zero {
            widget.set_values(map! {
                "icon" => Value::icon(api.get_icon("tasks")?),
                "count" => Value::number(number_of_tasks),
                "filter_name" => Value::text(filter.name.clone()),
                [if number_of_tasks == 0] "done" => Value::flag(),
                [if number_of_tasks == 1] "single" => Value::flag(),
            });

            widget.state = if number_of_tasks >= config.critical_threshold {
                State::Critical
            } else if number_of_tasks >= config.warning_threshold {
                State::Warning
            } else {
                State::Idle
            };

            api.set_widget(&widget).await?;
        } else {
            api.hide().await?;
        }

        select! {
            _ = sleep(config.interval.0) =>(),
            _ = updates.next() => (),
            event = api.event() => match event {
                Action(a) if a == "next_filter" => {
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
struct Filter {
    pub name: String,
    pub filter: String,
}

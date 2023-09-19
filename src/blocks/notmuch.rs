//! Count of notmuch messages
//!
//! This block queries a notmuch database and displays the count of messages.
//!
//! The simplest configuration will return the total count of messages in the notmuch database stored at $HOME/.mail
//!
//! Note that you need to enable `notmuch` feature to use this block:
//! ```sh
//! cargo build --release --features notmuch
//! ```
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `format` | A string to customise the output of this block. See below for available placeholders. | `" $icon $count "`
//! `maildir` | Path to the directory containing the notmuch database. Supports path expansions e.g. `~`. | `~/.mail`
//! `query` | Query to run on the database. | `""`
//! `threshold_critical` | Mail count that triggers `critical` state. | `99999`
//! `threshold_warning` | Mail count that triggers `warning` state. | `99999`
//! `threshold_good` | Mail count that triggers `good` state. | `99999`
//! `threshold_info` | Mail count that triggers `info` state. | `99999`
//! `interval` | Update interval in seconds. | `10`
//!
//! Placeholder | Value                                      | Type   | Unit
//! ------------|--------------------------------------------|--------|-----
//! `icon`      | A static icon                              | Icon   | -
//! `count`     | Number of messages for the query           | Number | -
//!
//! # Example
//!
//! ```toml
//! [[block]]
//! block = "notmuch"
//! query = "tag:alert and not tag:trash"
//! threshold_warning = 1
//! threshold_critical = 10
//! [[block.click]]
//! button = "left"
//! update = true
//! ```
//!
//! # Icons Used
//! - `mail`

use super::prelude::*;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub format: FormatConfig,
    #[default(10.into())]
    pub interval: Seconds,
    #[default("~/.mail".into())]
    pub maildir: ShellString,
    pub query: String,
    #[default(u32::MAX)]
    pub threshold_warning: u32,
    #[default(u32::MAX)]
    pub threshold_critical: u32,
    #[default(u32::MAX)]
    pub threshold_info: u32,
    #[default(u32::MAX)]
    pub threshold_good: u32,
}

pub async fn run(config: &Config, api: &CommonApi) -> Result<()> {
    let format = config.format.with_default(" $icon $count ")?;

    let db = config.maildir.expand()?;
    let mut timer = config.interval.timer();

    loop {
        // TODO: spawn_blocking?
        let count = run_query(&db, &config.query).error("Failed to get count")?;

        let mut widget = Widget::new().with_format(format.clone());

        widget.set_values(map! {
            "icon" => Value::icon("mail"),
            "count" => Value::number(count)
        });

        widget.state = if count >= config.threshold_critical {
            State::Critical
        } else if count >= config.threshold_warning {
            State::Warning
        } else if count >= config.threshold_good {
            State::Good
        } else if count >= config.threshold_info {
            State::Info
        } else {
            State::Idle
        };

        api.set_widget(widget)?;

        tokio::select! {
            _ = timer.tick() => (),
            _ = api.wait_for_update_request() => (),
        }
    }
}

fn run_query(db_path: &str, query_string: &str) -> std::result::Result<u32, notmuch::Error> {
    let db = notmuch::Database::open_with_config(
        Some(db_path),
        notmuch::DatabaseMode::ReadOnly,
        None::<&str>,
        None,
    )?;
    let query = db.create_query(query_string)?;
    query.count_messages()
}

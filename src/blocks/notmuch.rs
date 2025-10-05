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

use inotify::{Inotify, WatchMask};

use super::prelude::*;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub format: FormatConfig,
    /// Path to the notmuch database.
    ///
    /// Defaults to the database used by the notmuch CLI tool.
    pub database: Option<ShellString>,
    /// Database profile. Cannot be specified at the same time as `database`.
    ///
    /// Defaults to the profile used by the notmuch CLI tool.
    pub profile: Option<String>,
    /// The notmuch query to count.
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

    if config.database.is_some() && config.profile.is_some() {
        return Err(Error::new(
            "cannot specify both a notmuch database and a notmuch profile",
        ));
    }

    let profile = config.profile.as_deref();

    let db_path = config.database.as_ref().map(|p| p.expand()).transpose()?;
    let db_path: Option<&str> = db_path.as_deref();
    let notify = Inotify::init().error("Failed to start inotify")?;

    {
        let lock_path = open_database(db_path, profile)
            .error("failed to open the notmuch database")?
            .path()
            .join("xapian/flintlock");
        notify
            .watches()
            .add(lock_path, WatchMask::CLOSE_WRITE)
            .error("failed to watch the notmuch database lock")?;
    }

    let mut updates = notify
        .into_event_stream([0; 1024])
        .error("Failed to create event stream")?;

    loop {
        // TODO: spawn_blocking?
        let count = run_query(db_path, profile, &config.query).error("Failed to get count")?;

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
            _ = updates.next_debounced() => (),
            _ = api.wait_for_update_request() => (),
        }
    }
}

fn open_database(
    db_path: Option<&str>,
    profile: Option<&str>,
) -> std::result::Result<notmuch::Database, notmuch::Error> {
    notmuch::Database::open_with_config(
        db_path,
        notmuch::DatabaseMode::ReadOnly,
        None::<&str>,
        profile,
    )
}

fn run_query(
    db_path: Option<&str>,
    profile: Option<&str>,
    query_string: &str,
) -> std::result::Result<u32, notmuch::Error> {
    let db = notmuch::Database::open_with_config(
        db_path,
        notmuch::DatabaseMode::ReadOnly,
        None::<&str>,
        profile,
    )?;
    let query = db.create_query(query_string)?;
    query.count_messages()
}

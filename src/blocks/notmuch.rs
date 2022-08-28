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
//! `maildir` | Path to the directory containing the notmuch database. Supports path expansions e.g. `~`. | `~/.mail`
//! `query` | Query to run on the database. | `""`
//! `threshold_critical` | Mail count that triggers `critical` state. | `99999`
//! `threshold_warning` | Mail count that triggers `warning` state. | `99999`
//! `threshold_good` | Mail count that triggers `good` state. | `99999`
//! `threshold_info` | Mail count that triggers `info` state. | `99999`
//! `name` | Label to show before the mail count. | `None`
//! `interval` | Update interval in seconds. | `10`
//!
//! # Example
//!
//! ```toml
//! [[block]]
//! block = "notmuch"
//! query = "tag:alert and not tag:trash"
//! threshold_warning = 1
//! threshold_critical = 10
//! name = "A"
//! ```
//!
//! # Icons Used
//! - `mail`

use super::prelude::*;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
struct NotmuchConfig {
    #[default(10.into())]
    interval: Seconds,
    #[default("~/.mail".into())]
    maildir: ShellString,
    query: String,
    #[default(u32::MAX)]
    threshold_warning: u32,
    #[default(u32::MAX)]
    threshold_critical: u32,
    #[default(u32::MAX)]
    threshold_info: u32,
    #[default(u32::MAX)]
    threshold_good: u32,
    name: Option<String>,
}

pub async fn run(config: toml::Value, mut api: CommonApi) -> Result<()> {
    let config = NotmuchConfig::deserialize(config).config_error()?;
    let mut widget = api.new_widget().with_icon("mail")?;

    let db = config.maildir.expand()?;
    let mut timer = config.interval.timer();

    loop {
        // TODO: spawn_blocking?
        let count = run_query(&db, &config.query).error("Failed to get count")?;

        widget.set_text(match &config.name {
            Some(s) => format!("{s}:{count}"),
            None => format!("{count}"),
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

        api.set_widget(&widget).await?;

        loop {
            tokio::select! {
                _ = timer.tick() => break,
                event = api.event() => match event {
                    UpdateRequest => break,
                    Click(click) => {
                        if click.button == MouseButton::Left {
                            break;
                        }
                    }
                }
            }
        }
    }
}

fn run_query(db_path: &str, query_string: &str) -> std::result::Result<u32, notmuch::Error> {
    let db = notmuch::Database::open(&db_path, notmuch::DatabaseMode::ReadOnly)?;
    let query = db.create_query(query_string)?;
    query.count_messages()
}

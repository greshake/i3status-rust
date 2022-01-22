//! Count of notmuch messages
//!
//! This block queries a notmuch database and displays the count of messages.
//!
//! The simplest configuration will return the total count of messages in the notmuch database stored at $HOME/.mail
//!
//! # Configuration
//!
//! Key | Values | Required | Default
//! ----|--------|----------|--------
//! `maildir` | Path to the directory containing the notmuch database. | No | `$HOME/.mail`
//! `query` | Query to run on the database. | No | `""`
//! `threshold_critical` | Mail count that triggers `critical` state. | No | `99999`
//! `threshold_warning` | Mail count that triggers `warning` state. | No | `99999`
//! `threshold_good` | Mail count that triggers `good` state. | No | `99999`
//! `threshold_info` | Mail count that triggers `info` state. | No | `99999`
//! `name` | Label to show before the mail count. | No | None
//! `interval` | Update interval in seconds. | No | `10`
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

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields, default)]
struct NotmuchConfig {
    interval: Seconds,
    maildir: ShellString,
    query: String,
    threshold_warning: u32,
    threshold_critical: u32,
    threshold_info: u32,
    threshold_good: u32,
    name: Option<String>,
}

impl Default for NotmuchConfig {
    fn default() -> Self {
        Self {
            interval: Seconds::new(10),
            maildir: ShellString::new("~/.mail"),
            query: "".into(),
            threshold_warning: std::u32::MAX,
            threshold_critical: std::u32::MAX,
            threshold_info: std::u32::MAX,
            threshold_good: std::u32::MAX,
            name: None,
        }
    }
}

pub async fn run(config: toml::Value, mut api: CommonApi) -> Result<()> {
    let mut events = api.get_events().await?;
    let config = NotmuchConfig::deserialize(config).config_error()?;
    api.set_icon("mail")?;

    let db = config.maildir.expand()?;

    loop {
        // TODO: spawn_blocking
        let count = run_query(&db, &config.query).error("Failed to get count")?;

        let text = match &config.name {
            Some(s) => format!("{}:{}", s, count),
            None => format!("{}", count),
        };
        api.set_text(text.into());

        api.set_state(if count >= config.threshold_critical {
            State::Critical
        } else if count >= config.threshold_warning {
            State::Warning
        } else if count >= config.threshold_good {
            State::Good
        } else if count >= config.threshold_info {
            State::Info
        } else {
            State::Idle
        });

        api.flush().await?;

        loop {
            tokio::select! {
                _ = sleep(config.interval.0) => break,
                Some(BlockEvent::Click(click)) = events.recv() => {
                    if click.button == MouseButton::Left {
                        break;
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

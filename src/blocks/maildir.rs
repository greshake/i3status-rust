//! Unread mail. Only supports maildir format.
//!
//! Note that you need to enable `maildir` feature to use this block:
//! ```sh
//! cargo build --release --features maildir
//! ```
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `format` | A string to customise the output of this block. See below for available placeholders. | `" $icon $status "`
//! `inboxes` | List of maildir inboxes to look for mails in. Supports path expansions e.g. `~`. | **Required**
//! `threshold_warning` | Number of unread mails where state is set to warning. | `1`
//! `threshold_critical` | Number of unread mails where state is set to critical. | `10`
//! `interval` | Update interval, in seconds. | `5`
//! `display_type` | Which part of the maildir to count: `"new"`, `"cur"`, or `"all"`. | `"new"`
//!
//! Placeholder  | Value                  | Type   | Unit
//! -------------|------------------------|--------|-----
//! `icon`       | A static icon          | Icon   | -
//! `status`     | Number of emails       | Number | -
//!
//! # Examples
//!
//! ```toml
//! [[block]]
//! block = "maildir"
//! interval = 60
//! inboxes = ["/home/user/mail/local", "/home/user/mail/gmail/Inbox"]
//! threshold_warning = 1
//! threshold_critical = 10
//! display_type = "new"
//! ```
//!
//! # Icons Used
//! - `mail`

use super::prelude::*;
use maildir::Maildir;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(default)]
pub struct Config {
    format: FormatConfig,
    #[default(5.into())]
    interval: Seconds,
    inboxes: Vec<String>,
    #[default(1)]
    threshold_warning: usize,
    #[default(10)]
    threshold_critical: usize,
    #[default(MailType::New)]
    display_type: MailType,
}

pub async fn run(mut config: Config, mut api: CommonApi) -> Result<()> {
    let mut widget = Widget::new().with_format(config.format.with_default(" $icon $status ")?);

    for inbox in &mut config.inboxes {
        *inbox = shellexpand::full(inbox)
            .error("Failed to expand string")?
            .to_string();
    }

    loop {
        let mut newmails = 0;
        for inbox in &config.inboxes {
            let isl: &str = &inbox[..];
            // TODO: spawn_blocking?
            let maildir = Maildir::from(isl);
            newmails += match config.display_type {
                MailType::New => maildir.count_new(),
                MailType::Cur => maildir.count_cur(),
                MailType::All => maildir.count_new() + maildir.count_cur(),
            };
        }
        widget.state = if newmails >= config.threshold_critical {
            State::Critical
        } else if newmails >= config.threshold_warning {
            State::Warning
        } else {
            State::Idle
        };
        widget.set_values(map!(
            "icon" => Value::icon(api.get_icon("mail")?),
            "status" => Value::number(newmails)
        ));
        api.set_widget(&widget).await?;

        select! {
            _ = sleep(config.interval.0) => (),
            _ = api.wait_for_update_request() => (),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum MailType {
    New,
    Cur,
    All,
}

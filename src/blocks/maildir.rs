//! Unread mail. Only supports maildir format.
//!
//! Note that you need to enable `maildir` feature to use this block:
//! ```sh
//! cargo build --release --features maildir
//! ```
//!
//! # Configuration
//!
//! Key | Values | Required | Default
//! ----|--------|----------|--------
//! `inboxes` | List of maildir inboxes to look for mails in. | Yes | None
//! `threshold_warning` | Number of unread mails where state is set to warning. | No | `1`
//! `threshold_critical` | Number of unread mails where state is set to critical. | No | `10`
//! `interval` | Update interval, in seconds. | No | `5`
//! `display_type` | Which part of the maildir to count: `"new"`, `"cur"`, or `"all"`. | No | `"new"`
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
//! # TODO
//! - Add `format` option.
//!
//! # Icons Used
//! - `mail`

use super::prelude::*;
use maildir::Maildir;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
struct MaildirConfig {
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

pub async fn run(config: toml::Value, mut api: CommonApi) -> Result<()> {
    let config = MaildirConfig::deserialize(config).config_error()?;
    let mut widget = api.new_widget().with_icon("mail")?;

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
        widget.set_text(newmails.to_string());
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

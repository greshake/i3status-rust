//! Unread mail. Only supports maildir format.
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
//! # Icons Used
//! - `mail`

use super::prelude::*;
use maildir::Maildir;

//TODO add `format`
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields, default)]
struct MaildirConfig {
    interval: Seconds,
    inboxes: Vec<String>,
    threshold_warning: usize,
    threshold_critical: usize,
    display_type: MailType,
}

impl Default for MaildirConfig {
    fn default() -> Self {
        Self {
            interval: Seconds::new(5),
            inboxes: Vec::new(),
            threshold_warning: 1,
            threshold_critical: 10,
            display_type: MailType::New,
        }
    }
}

pub async fn run(config: toml::Value, mut api: CommonApi) -> Result<()> {
    let config = MaildirConfig::deserialize(config).config_error()?;
    api.set_icon("mail")?;

    let mut timer = config.interval.timer();

    loop {
        let mut newmails = 0;
        for inbox in &config.inboxes {
            let isl: &str = &inbox[..];
            // TODO: spawn_blocking
            let maildir = Maildir::from(isl);
            newmails += match config.display_type {
                MailType::New => maildir.count_new(),
                MailType::Cur => maildir.count_cur(),
                MailType::All => maildir.count_new() + maildir.count_cur(),
            };
        }
        let mut state = State::Idle;
        if newmails >= config.threshold_critical {
            state = State::Critical;
        } else if newmails >= config.threshold_warning {
            state = State::Warning;
        }
        api.set_state(state);
        api.set_text(newmails.to_string().into());
        api.flush().await?;
        timer.tick().await;
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum MailType {
    New,
    Cur,
    All,
}

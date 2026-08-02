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
//! `inboxes` | List of maildir inboxes to look for mails in. Supports path/glob expansions (e.g. `~` and `*`). | **Required**
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
//! inboxes = ["~/mail/local", "~/maildir/account1/*"]
//! threshold_warning = 1
//! threshold_critical = 10
//! display_type = "new"
//! ```
//!
//! # Icons Used
//! - `mail`

use super::prelude::*;
use maildir::Maildir;
use std::path::PathBuf;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub format: FormatConfig,
    #[default(5.into())]
    pub interval: Seconds,
    pub inboxes: Vec<String>,
    #[default(1)]
    pub threshold_warning: usize,
    #[default(10)]
    pub threshold_critical: usize,
    #[default(MailType::New)]
    pub display_type: MailType,
}

fn expand_inbox(inbox: &str) -> Result<impl Iterator<Item = PathBuf>> {
    let expanded = shellexpand::full(inbox).error("Failed to expand inbox")?;
    let paths = glob::glob(&expanded).error("Glob expansion failed")?;
    Ok(paths.filter_map(|p| p.ok()))
}

pub(crate) fn prepare(config: &Config) -> Result<Arc<BlockPlan>> {
    BlockPlan::new(vec![
        OutputPlan::new("main", config.format.with_default(" $icon $status ")?)
            .icon("icon", IconChoices::one("mail")),
    ])
}

pub async fn run(config: &Config, api: &CommonApi, plan: &Arc<BlockPlan>) -> Result<()> {
    let output_main = plan.output("main")?;

    let mut inboxes = Vec::with_capacity(config.inboxes.len());
    for inbox in &config.inboxes {
        inboxes.extend(expand_inbox(inbox)?.map(Maildir::from));
    }

    loop {
        let mut newmails = 0;
        for inbox in &inboxes {
            // TODO: spawn_blocking?
            newmails += match config.display_type {
                MailType::New => inbox.count_new(),
                MailType::Cur => inbox.count_cur(),
                MailType::All => inbox.count_new() + inbox.count_cur(),
            };
        }

        let mut widget = output_main.new_widget();
        widget.state = if newmails >= config.threshold_critical {
            State::Critical
        } else if newmails >= config.threshold_warning {
            State::Warning
        } else {
            State::Idle
        };
        widget.set_values(map!(
            "icon" => output_main.icon_value("icon")?,
            "status" => Value::number(newmails)
        ));
        api.set_widget(widget)?;

        select! {
            _ = sleep(config.interval.0) => (),
            _ = api.wait_for_update_request() => (),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MailType {
    New,
    Cur,
    All,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_declares_main_output_with_mail_icon() {
        let plan = prepare(&Config::default()).unwrap();
        let declared: Vec<_> = plan.outputs().map(|o| o.id()).collect();
        assert_eq!(declared, ["main"]);
        let output = plan.output("main").unwrap();
        assert_eq!(output.single_icon("icon").unwrap(), "mail");
    }


    #[test]
    fn custom_format_is_respected() {
        let config = Config {
            format: " $status ".parse().unwrap(),
            ..Config::default()
        };
        let plan = prepare(&config).unwrap();
        let output = plan.output("main").unwrap();
        assert!(output.format().contains_key("status"));
        assert!(!output.format().contains_key("icon"));
    }
}

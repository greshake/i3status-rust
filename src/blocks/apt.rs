//! Pending updates available for your Debian/Ubuntu based system
//!
//! Behind the scenes this uses `apt`, and in order to run it without root privileges i3status-rust will create its own package database in `/tmp/i3rs-apt/` which may take up several MB or more. If you have a custom apt config then this block may not work as expected - in that case please open an issue.
//!
//! Tip: You can grab the list of available updates using `APT_CONFIG=/tmp/i3rs-apt/apt.conf apt list --upgradable`
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `interval` | Update interval in seconds. | `600`
//! `format` | A string to customise the output of this block. See below for available placeholders. | `" $icon $count.eng(w:1) "`
//! `format_singular` | Same as `format`, but for when exactly one update is available. | `" $icon $count.eng(w:1) "`
//! `format_up_to_date` | Same as `format`, but for when no updates are available. | `" $icon $count.eng(w:1) "`
//! `warning_updates_regex` | Display block as warning if updates matching regex are available. | `None`
//! `critical_updates_regex` | Display block as critical if updates matching regex are available. | `None`
//! `ignore_updates_regex` | Doesn't include updates matching regex in the count. | `None`
//! `ignore_phased_updates` | Doesn't include potentially held back phased updates in the count. | `false`
//!
//! Placeholder | Value                       | Type   | Unit
//! ------------|-----------------------------|--------|------
//! `icon`      | A static icon               | Icon   | -
//! `count`     | Number of updates available | Number | -
//!
//! # Example
//!
//! Update the list of pending updates every thirty minutes (1800 seconds):
//!
//! ```toml
//! [[block]]
//! block = "apt"
//! interval = 1800
//! format = " $icon $count updates available "
//! format_singular = " $icon One update available "
//! format_up_to_date = " $icon system up to date "
//! critical_updates_regex = "(linux|linux-lts|linux-zen)"
//! [[block.click]]
//! # shows dmenu with cached available updates. Any dmenu alternative should also work.
//! button = "left"
//! cmd = "APT_CONFIG=/tmp/i3rs-apt/apt.conf apt list --upgradable | tail -n +2 | rofi -dmenu"
//! [[block.click]]
//! # Updates the block on right click
//! button = "right"
//! update = true
//! ```
//!
//! # Icons Used
//!
//! - `update`

use regex::Regex;

use super::{
    packages::{apt::Apt, has_matching_update, Backend},
    prelude::*,
};

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    #[default(600.into())]
    pub interval: Seconds,
    pub format: FormatConfig,
    pub format_singular: FormatConfig,
    pub format_up_to_date: FormatConfig,
    pub warning_updates_regex: Option<String>,
    pub critical_updates_regex: Option<String>,
    pub ignore_updates_regex: Option<String>,
    pub ignore_phased_updates: bool,
}

pub async fn run(config: &Config, api: &CommonApi) -> Result<()> {
    let format = config.format.with_default(" $icon $count.eng(w:1) ")?;
    let format_singular = config
        .format_singular
        .with_default(" $icon $count.eng(w:1) ")?;
    let format_up_to_date = config
        .format_up_to_date
        .with_default(" $icon $count.eng(w:1) ")?;

    let warning_updates_regex = config
        .warning_updates_regex
        .as_deref()
        .map(Regex::new)
        .transpose()
        .error("invalid warning updates regex")?;
    let critical_updates_regex = config
        .critical_updates_regex
        .as_deref()
        .map(Regex::new)
        .transpose()
        .error("invalid critical updates regex")?;
    let ignore_updates_regex = config
        .ignore_updates_regex
        .as_deref()
        .map(Regex::new)
        .transpose()
        .error("invalid ignore updates regex")?;

    let backend = Apt::new(config.ignore_phased_updates).await?;

    loop {
        let mut widget = Widget::new();
        let mut updates = backend.get_updates_list().await?;
        if let Some(regex) = ignore_updates_regex.clone() {
            updates.retain(|u| !regex.is_match(u));
        }
        let count = updates.len();

        widget.set_format(match count {
            0 => format_up_to_date.clone(),
            1 => format_singular.clone(),
            _ => format.clone(),
        });
        widget.set_values(map!(
            "count" => Value::number(count),
            "icon" => Value::icon("update"),
        ));

        let warning = warning_updates_regex
            .as_ref()
            .is_some_and(|regex| has_matching_update(&updates, regex));
        let critical = critical_updates_regex
            .as_ref()
            .is_some_and(|regex| has_matching_update(&updates, regex));
        widget.state = match count {
            0 => State::Idle,
            _ => {
                if critical {
                    State::Critical
                } else if warning {
                    State::Warning
                } else {
                    State::Info
                }
            }
        };

        api.set_widget(widget)?;

        select! {
            _ = sleep(config.interval.0) => (),
            _ = api.wait_for_update_request() => (),
        }
    }
}

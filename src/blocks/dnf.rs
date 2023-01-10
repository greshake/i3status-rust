//! Pending updates available for your Fedora system
//!
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
//!
//! Placeholder | Value                       | Type | Unit
//! ------------|-----------------------------|--------|-----
//! `icon`      | A static icon               | Icon   | -
//! `count`     | Number of updates available | Number | -
//!
//! # Example
//!
//! Update the list of pending updates every thirty minutes (1800 seconds):
//!
//! ```toml
//! [[block]]
//! block = "dnf"
//! interval = 1800
//! format = " $icon $count.eng(1) updates available "
//! format_singular = " $icon One update available "
//! format_up_to_date = " $icon system up to date "
//! critical_updates_regex = "(linux|linux-lts|linux-zen)"
//! [[block.click]]
//! # shows dmenu with cached available updates. Any dmenu alternative should also work.
//! button = "left"
//! cmd = "dnf list -q --upgrades | tail -n +2 | rofi -dmenu"
//! ```
//!
//! # Icons Used
//!
//! - `update`

use super::prelude::*;
use regex::Regex;
use tokio::process::Command;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(default)]
pub struct Config {
    #[default(600.into())]
    interval: Seconds,
    format: FormatConfig,
    format_singular: FormatConfig,
    format_up_to_date: FormatConfig,
    warning_updates_regex: Option<String>,
    critical_updates_regex: Option<String>,
}

pub async fn run(config: Config, mut api: CommonApi) -> Result<()> {
    let mut widget = Widget::new();

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

    loop {
        let updates = get_updates_list().await?;
        let count = get_update_count(&updates);

        widget.set_format(match count {
            0 => format_up_to_date.clone(),
            1 => format_singular.clone(),
            _ => format.clone(),
        });
        widget.set_values(map!(
            "icon" => Value::icon(api.get_icon("update")?),
            "count" => Value::number(count)
        ));

        let warning = warning_updates_regex
            .as_ref()
            .map_or(false, |regex| has_matching_update(&updates, regex));
        let critical = critical_updates_regex
            .as_ref()
            .map_or(false, |regex| has_matching_update(&updates, regex));
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

        api.set_widget(&widget).await?;

        select! {
            _ = sleep(config.interval.0) => (),
            _ = api.wait_for_update_request() => (),
        }
    }
}

async fn get_updates_list() -> Result<String> {
    let stdout = Command::new("sh")
        .env("LC_LANG", "C")
        .args(["-c", "dnf check-update -q --skip-broken"])
        .output()
        .await
        .error("Failed to run dnf check-update")?
        .stdout;
    String::from_utf8(stdout).error("dnf produced non-UTF8 output")
}

fn get_update_count(updates: &str) -> usize {
    updates.lines().filter(|line| line.len() > 1).count()
}

fn has_matching_update(updates: &str, regex: &Regex) -> bool {
    updates.lines().any(|line| regex.is_match(line))
}

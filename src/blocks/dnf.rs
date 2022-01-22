//! Pending updates available for your Fedora system
//!
//!
//! # Configuration
//!
//! Key | Values | Required | Default
//! ----|--------|----------|--------
//! `interval` | Update interval in seconds. | No | `600`
//! `format` | A string to customise the output of this block. See below for available placeholders. | No | `"$count.eng(1)"`
//! `format_singular` | Same as `format`, but for when exactly one update is available. | No | `"$count.eng(1)"`
//! `format_up_to_date` | Same as `format`, but for when no updates are available. | No | `"$count.eng(1)"`
//! `warning_updates_regex` | Display block as warning if updates matching regex are available. | No | `None`
//! `critical_updates_regex` | Display block as critical if updates matching regex are available. | No | `None`
//! `hide_when_uptodate` | Hides the block when there are no updates available | `false`
//!
//! Key | Value | Type | Unit
//! ----|-------|------|-----
//! `count` | Number of updates available | Number | -
//!
//! # Example
//!
//! Update the list of pending updates every thirty minutes (1800 seconds):
//!
//! ```toml
//! [[block]]
//! block = "dnf"
//! interval = 1800
//! format = "$count.eng(1) updates available"
//! format_singular = "One update available"
//! format_up_to_date = "system up to date"
//! critical_updates_regex = "(linux|linux-lts|linux-zen)"
//! # shows dmenu with cached available updates. Any dmenu alternative should also work.
//! on_click = "dnf list -q --upgrades | tail -n +2 | rofi -dmenu"
//! ```
//!
//! # Icons Used
//!
//! - `update`

use super::prelude::*;
use regex::Regex;
use tokio::process::Command;

#[derive(Deserialize, Debug, Derivative)]
#[serde(deny_unknown_fields, default)]
#[derivative(Default)]
struct DnfConfig {
    #[derivative(Default(value = "600.into()"))]
    interval: Seconds,
    format: FormatConfig,
    format_singular: FormatConfig,
    format_up_to_date: FormatConfig,
    warning_updates_regex: Option<String>,
    critical_updates_regex: Option<String>,
    hide_when_uptodate: bool,
}

pub async fn run(config: toml::Value, mut api: CommonApi) -> Result<()> {
    let config = DnfConfig::deserialize(config).config_error()?;
    let mut events = api.get_events().await?;
    api.set_icon("update")?;

    let format = config.format.with_default("$count.eng(1)")?;
    let format_singular = config.format_singular.with_default("$count.eng(1)")?;
    let format_up_to_date = config.format_up_to_date.with_default("$count.eng(1)")?;

    let warning_updates_regex = match config.warning_updates_regex {
        None => None,
        Some(regex_str) => {
            let regex = Regex::new(&regex_str).error("invalid warning updates regex")?;
            Some(regex)
        }
    };
    let critical_updates_regex = match config.critical_updates_regex {
        None => None,
        Some(regex_str) => {
            let regex = Regex::new(&regex_str).error("invalid critical updates regex")?;
            Some(regex)
        }
    };

    loop {
        let updates = get_updates_list().await?;
        let count = get_update_count(&updates);

        if count == 0 && config.hide_when_uptodate {
            api.hide();
        } else {
            api.show();

            match count {
                0 => api.set_format(format_up_to_date.clone()),
                1 => api.set_format(format_singular.clone()),
                _ => api.set_format(format.clone()),
            }
            api.set_values(map!("count" => Value::number(count)));

            let warning = warning_updates_regex
                .as_ref()
                .map_or(false, |regex| has_matching_update(&updates, regex));
            let critical = critical_updates_regex
                .as_ref()
                .map_or(false, |regex| has_matching_update(&updates, regex));
            api.set_state(match count {
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
            });
        }

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

async fn get_updates_list() -> Result<StdString> {
    let stdout = Command::new("sh")
        .env("LC_LANG", "C")
        .args(&["-c", "dnf check-update -q --skip-broken"])
        .output()
        .await
        .error("Failed to run dnf check-update")?
        .stdout;
    StdString::from_utf8(stdout).error("dnf produced non-UTF8 output")
}

fn get_update_count(updates: &str) -> usize {
    updates.lines().filter(|line| line.len() > 1).count()
}

fn has_matching_update(updates: &str, regex: &Regex) -> bool {
    updates.lines().any(|line| regex.is_match(line))
}

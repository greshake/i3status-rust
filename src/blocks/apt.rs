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

use std::env;
use std::process::Stdio;

use regex::Regex;

use tokio::fs::{create_dir_all, File};
use tokio::process::Command;

use super::prelude::*;

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

    let mut cache_dir = env::temp_dir();
    cache_dir.push("i3rs-apt");
    if !cache_dir.exists() {
        create_dir_all(&cache_dir)
            .await
            .error("Failed to create temp dir")?;
    }

    let apt_config = format!(
        "Dir::State \"{}\";\n
             Dir::State::lists \"lists\";\n
             Dir::Cache \"{}\";\n
             Dir::Cache::srcpkgcache \"srcpkgcache.bin\";\n
             Dir::Cache::pkgcache \"pkgcache.bin\";",
        cache_dir.display(),
        cache_dir.display(),
    );

    let mut config_file = cache_dir;
    config_file.push("apt.conf");
    let mut file = File::create(&config_file)
        .await
        .error("Failed to create config file")?;
    file.write_all(apt_config.as_bytes())
        .await
        .error("Failed to write to config file")?;

    loop {
        let updates = get_updates_list(config_file.to_str().unwrap()).await?;
        let count = get_update_count(&updates);

        widget.set_format(match count {
            0 => format_up_to_date.clone(),
            1 => format_singular.clone(),
            _ => format.clone(),
        });
        widget.set_values(map!(
            "count" => Value::number(count),
            "icon" => Value::icon(api.get_icon("update")?)
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

async fn get_updates_list(config_path: &str) -> Result<String> {
    Command::new("apt")
        .env("APT_CONFIG", config_path)
        .args(["update"])
        .stdout(Stdio::null())
        .stdin(Stdio::null())
        .spawn()
        .error("Failed to run `apt update`")?
        .wait()
        .await
        .error("Failed to run `apt update`")?;
    let stdout = Command::new("apt")
        .env("APT_CONFIG", config_path)
        .args(["list", "--upgradable"])
        .output()
        .await
        .error("Problem running apt command")?
        .stdout;
    String::from_utf8(stdout).error("apt produced non-UTF8 output")
}

fn get_update_count(updates: &str) -> usize {
    updates
        .lines()
        .filter(|line| line.contains("[upgradable"))
        .count()
}

fn has_matching_update(updates: &str, regex: &Regex) -> bool {
    updates.lines().any(|line| regex.is_match(line))
}

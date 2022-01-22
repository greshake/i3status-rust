//! Pending updates available on pacman or an AUR helper.
//!
//! Requires fakeroot to be installed (only required for pacman).
//!
//! Tip: You can grab the list of available updates using `fakeroot pacman -Qu --dbpath /tmp/checkup-db-yourusername/`. If you have the CHECKUPDATES_DB env var set on your system then substitute that dir instead of /tmp/checkup-db-yourusername.
//!
//! Tip: On Arch Linux you can setup a `pacman` hook to signal i3status-rs to update after packages have been upgraded, so you won't have stale info in your pacman block. Create `/usr/share/libalpm/hooks/i3status.hook` with the below contents:
//!
//! Note: `pikaur` may hang the whole block if there is no internet connectivity. In that case, try a different AUR helper.
//! ```ini
//! [Trigger]
//! Operation = Upgrade
//! Type = Package
//! Target = *
//!
//! [Action]
//! When = PostTransaction
//! Exec = /usr/bin/pkill -SIGUSR1 i3status-rs
//! ```
//!
//! # Configuration
//!
//! Key | Values | Required | Default
//! ----|--------|----------|--------
//! `interval` | Update interval, in seconds. | No | `600`
//! `format` | A string to customise the output of this block. See below for available placeholders. | No | `"$pacman.eng(1)"`
//! `format_singular` | Same as `format` but for when exactly one update is available. | No | `"$pacman.eng(1)"`
//! `format_up_to_date` | Same as `format` but for when no updates are available. | No | `"$pacman.eng(1)"`
//! `warning_updates_regex` | Display block as warning if updates matching regex are available. | No | `None`
//! `critical_updates_regex` | Display block as critical if updates matching regex are available. | No | `None`
//! `aur_command` | AUR command to check available updates, which outputs in the same format as pacman. e.g. `yay -Qua` | if `{both}` or `{aur}` are used. | `None`
//! `hide_when_uptodate` | Hides the block when there are no updates available | `false`
//!
//!  Key    | Value | Type | Unit
//! --------|-------|------|-----
//! `pacman`| Number of updates available according to `pacman` | Number | -
//! `aur`   | Number of updates available according to `<aur_command>` | Number | -
//! `both`  | Cumulative number of updates available according to `pacman` and `<aur_command>` | Number | -
//!
//! # Examples
//!
//! Update the list of pending updates every ten minutes (600 seconds):
//!
//! Update interval should be set appropriately as to not exceed the AUR's daily rate limit.
//!
//! pacman only config:
//!
//! ```toml
//! [[block]]
//! block = "pacman"
//! interval = 600
//! format = "$pacman updates available"
//! format_singular = "$pacman update available"
//! format_up_to_date = "system up to date"
//! critical_updates_regex = "(linux|linux-lts|linux-zen)"
//! # pop-up a menu showing the available updates. Replace wofi with your favourite menu command.
//! on_click = "fakeroot pacman -Qu --dbpath /tmp/checkup-db-yourusername/ | wofi --show dmenu"
//! ```
//!
//! pacman only config using warnings with ZFS modules:
//!
//! ```toml
//! [[block]]
//! block = "pacman"
//! interval = 600
//! format = "$pacman updates available"
//! format_singular = "$pacman update available"
//! format_up_to_date = "system up to date"
//! # If a linux update is availble, but no ZFS package, it won't be possible to
//! # actually perform a system upgrade, so we show a warning.
//! warning_updates_regex = "(linux|linux-lts|linux-zen)"
//! # If ZFS is available, we know that we can and should do an upgrade, so we show
//! # the status as critical.
//! critical_updates_regex = "(zfs|zfs-lts)"
//! ```
//!
//! pacman and AUR helper config:
//!
//! ```toml
//! [[block]]
//! block = "pacman"
//! interval = 600
//! format = "$pacman + $aur = $both updates available"
//! format_singular = "$both update available"
//! format_up_to_date = "system up to date"
//! critical_updates_regex = "(linux|linux-lts|linux-zen)"
//! # aur_command should output available updates to stdout (ie behave as echo -ne "update\n")
//! aur_command = "yay -Qua"
//! ```
//!
//! # Icons Used
//!
//! - `update`

use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use std::process::Stdio;

use regex::Regex;

use tokio::fs::{create_dir_all, symlink};
use tokio::process::Command;

use super::prelude::*;
use crate::util::has_command;

#[derive(Deserialize, Debug, Derivative)]
#[serde(deny_unknown_fields, default)]
#[derivative(Default)]
struct PacmanConfig {
    #[derivative(Default(value = "600.into()"))]
    interval: Seconds,
    format: FormatConfig,
    format_singular: FormatConfig,
    format_up_to_date: FormatConfig,
    warning_updates_regex: Option<String>,
    critical_updates_regex: Option<String>,
    aur_command: Option<String>,
    hide_when_uptodate: bool,
}

pub async fn run(config: toml::Value, mut api: CommonApi) -> Result<()> {
    let config = PacmanConfig::deserialize(config).config_error()?;
    let mut events = api.get_events().await?;
    api.set_icon("update")?;

    let format = config.format.with_default("$pacman.eng(1)")?;
    let format_singular = config.format_singular.with_default("$pacman.eng(1)")?;
    let format_up_to_date = config.format_up_to_date.with_default("$pacman.eng(1)")?;

    macro_rules! any_format_contains {
        ($name:expr) => {
            format.contains_key($name)
                || format_singular.contains_key($name)
                || format_up_to_date.contains_key($name)
        };
    }
    let aur = any_format_contains!("aur");
    let pacman = any_format_contains!("pacman");
    let both = any_format_contains!("both");
    let watched = if both || (pacman && aur) {
        Watched::Both(
            config
                .aur_command
                .error("$aur or $both found in format string but no aur_command supplied")?,
        )
    } else if pacman && !aur {
        Watched::Pacman
    } else if !pacman && aur {
        Watched::Aur(
            config
                .aur_command
                .error("$aur or $both found in format string but no aur_command supplied")?,
        )
    } else {
        Watched::None
    };

    if matches!(watched, Watched::Pacman | Watched::Both(_)) {
        check_fakeroot_command_exists().await?;
    }

    let warning_updates_regex = match config.warning_updates_regex {
        None => None, // no regex configured
        Some(regex_str) => {
            let regex = Regex::new(&regex_str).error("invalid warning updates regex")?;
            Some(regex)
        }
    };
    let critical_updates_regex = match config.critical_updates_regex {
        None => None, // no regex configured
        Some(regex_str) => {
            let regex = Regex::new(&regex_str).error("invalid critical updates regex")?;
            Some(regex)
        }
    };

    loop {
        let (values, warning, critical, total) = match &watched {
            Watched::Pacman => {
                let updates = api.recoverable(get_pacman_available_updates, "X").await?;
                let count = get_update_count(&updates);
                let values = map!("pacman" => Value::number(count));
                let warning = warning_updates_regex
                    .as_ref()
                    .map_or(false, |regex| has_matching_update(&updates, regex));
                let critical = critical_updates_regex
                    .as_ref()
                    .map_or(false, |regex| has_matching_update(&updates, regex));
                (values, warning, critical, count)
            }
            Watched::Aur(aur_command) => {
                let updates = api
                    .recoverable(|| get_aur_available_updates(aur_command), "X")
                    .await?;
                let count = get_update_count(&updates);
                let values = map!(
                    "aur" => Value::number(count)
                );
                let warning = warning_updates_regex
                    .as_ref()
                    .map_or(false, |regex| has_matching_update(&updates, regex));
                let critical = critical_updates_regex
                    .as_ref()
                    .map_or(false, |regex| has_matching_update(&updates, regex));
                (values, warning, critical, count)
            }
            Watched::Both(aur_command) => {
                let (pacman_updates, aur_updates) = api
                    .recoverable(
                        || async {
                            tokio::try_join!(
                                get_pacman_available_updates(),
                                get_aur_available_updates(aur_command)
                            )
                        },
                        "X",
                    )
                    .await?;
                let pacman_count = get_update_count(&pacman_updates);
                let aur_count = get_update_count(&aur_updates);
                let values = map! {
                    "pacman" => Value::number(pacman_count),
                    "aur" =>    Value::number(aur_count),
                    "both" =>   Value::number(pacman_count + aur_count),
                };
                let warning = warning_updates_regex.as_ref().map_or(false, |regex| {
                    has_matching_update(&aur_updates, regex)
                        || has_matching_update(&pacman_updates, regex)
                });
                let critical = critical_updates_regex.as_ref().map_or(false, |regex| {
                    has_matching_update(&aur_updates, regex)
                        || has_matching_update(&pacman_updates, regex)
                });
                (values, warning, critical, pacman_count + aur_count)
            }
            Watched::None => (HashMap::new(), false, false, 0),
        };

        if total == 0 && config.hide_when_uptodate {
            api.hide();
        } else {
            api.show();

            match total {
                0 => api.set_format(format_up_to_date.clone()),
                1 => api.set_format(format_singular.clone()),
                _ => api.set_format(format.clone()),
            }
            api.set_values(values);
            api.set_state(match total {
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

#[derive(Debug, PartialEq, Eq)]
enum Watched {
    None,
    Pacman,
    Aur(String),
    Both(String),
}

async fn check_fakeroot_command_exists() -> Result<()> {
    if !has_command("fakeroot").await? {
        Err(Error::new("fakeroot not found"))
    } else {
        Ok(())
    }
}

fn get_updates_db_dir() -> Result<PathBuf> {
    match env::var_os("CHECKUPDATES_DB") {
        Some(val) => Ok(val.into()),
        None => {
            let mut path = env::temp_dir();
            let user = env::var("USER").unwrap_or_default();
            path.push(format!("checkup-db-{}", user));
            Ok(path)
        }
    }
}

async fn get_pacman_available_updates() -> Result<StdString> {
    let updates_db = get_updates_db_dir()?;

    // Determine pacman database path
    let db_path = env::var_os("DBPath")
        .map(Into::into)
        .unwrap_or_else(|| PathBuf::from("/var/lib/pacman/"));

    // Create the determined `checkup-db` path recursively
    create_dir_all(&updates_db).await.or_error(|| {
        format!(
            "Failed to create checkup-db directory at '{}'",
            updates_db.display()
        )
    })?;

    // Create symlink to local cache in `checkup-db` if required
    let local_cache = updates_db.join("local");
    if !local_cache.exists() {
        symlink(db_path.join("local"), local_cache)
            .await
            .error("Failed to created required symlink")?;
    }

    // Update database
    let status = Command::new("sh")
        .env("LC_ALL", "C")
        .args([
            "-c",
            &format!(
                "fakeroot -- pacman -Sy --dbpath \"{}\" --logfile /dev/null",
                updates_db.display()
            ),
        ])
        .stdout(Stdio::null())
        .status()
        .await
        .error("Failed to run command")?;
    if !status.success() {
        return Err(Error::new("pacman -Sy exited with non zero exit status"));
    }

    let stdout = Command::new("sh")
        .env("LC_ALL", "C")
        .args([
            "-c",
            &format!("fakeroot pacman -Qu --dbpath \"{}\"", updates_db.display()),
        ])
        .output()
        .await
        .error("There was a problem running the pacman commands")?
        .stdout;

    StdString::from_utf8(stdout).error("Pacman produced non-UTF8 output")
}

async fn get_aur_available_updates(aur_command: &str) -> Result<StdString> {
    let stdout = Command::new("sh")
        .args(&["-c", aur_command])
        .output()
        .await
        .or_error(|| format!("aur command: {} failed", aur_command))?
        .stdout;
    StdString::from_utf8(stdout)
        .error("There was a problem while converting the aur command output to a string")
}

fn get_update_count(updates: &str) -> usize {
    updates
        .lines()
        .filter(|line| !line.contains("[ignored]"))
        .count()
}

fn has_matching_update(updates: &str, regex: &Regex) -> bool {
    updates.lines().any(|line| regex.is_match(line))
}

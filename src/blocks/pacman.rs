use std::env;
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use crossbeam_channel::Sender;
use regex::Regex;
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::SharedConfig;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::formatting::value::Value;
use crate::formatting::FormatTemplate;
use crate::protocol::i3bar_event::{I3BarEvent, MouseButton};
use crate::scheduler::Task;
use crate::util::has_command;
use crate::widgets::text::TextWidget;
use crate::widgets::{I3BarWidget, State};

pub struct Pacman {
    id: usize,
    output: TextWidget,
    update_interval: Duration,
    format: FormatTemplate,
    format_singular: FormatTemplate,
    format_up_to_date: FormatTemplate,
    warning_updates_regex: Option<Regex>,
    critical_updates_regex: Option<Regex>,
    watched: Watched,
    uptodate: bool,
    hide_when_uptodate: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Watched {
    None,
    Pacman,
    /// cf `Pacman::aur_command`
    AUR(String),
    /// cf `Pacman::aur_command`
    Both(String),
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct PacmanConfig {
    /// Update interval in seconds
    #[serde(deserialize_with = "deserialize_duration")]
    pub interval: Duration,

    /// Format override
    pub format: FormatTemplate,

    /// Alternative format override for when exactly 1 update is available
    pub format_singular: FormatTemplate,

    /// Alternative format override for when no updates are available
    pub format_up_to_date: FormatTemplate,

    /// Indicate a `warning` state for the block if any pending update match the
    /// following regex. Default behaviour is that no package updates are deemed
    /// warning
    pub warning_updates_regex: Option<String>,

    /// Indicate a `critical` state for the block if any pending update match the following regex.
    /// Default behaviour is that no package updates are deemed critical
    pub critical_updates_regex: Option<String>,

    /// Optional AUR command, listing available updates
    pub aur_command: Option<String>,

    pub hide_when_uptodate: bool,
}

impl Default for PacmanConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(600),
            format: FormatTemplate::default(),
            format_singular: FormatTemplate::default(),
            format_up_to_date: FormatTemplate::default(),
            warning_updates_regex: None,
            critical_updates_regex: None,
            aur_command: None,
            hide_when_uptodate: false,
        }
    }
}

impl PacmanConfig {
    fn watched(
        format: &FormatTemplate,
        format_singular: &FormatTemplate,
        format_up_to_date: &FormatTemplate,
        aur_command: Option<String>,
    ) -> Result<Watched> {
        macro_rules! any_format_contains {
            ($name:expr) => {
                format.contains($name)
                    || format_singular.contains($name)
                    || format_up_to_date.contains($name)
            };
        }
        let aur = any_format_contains!("aur");
        let pacman = any_format_contains!("pacman") || any_format_contains!("count");
        let both = any_format_contains!("both");
        if both || (pacman && aur) {
            let aur_command = aur_command.block_error(
                "pacman",
                "{aur} or {both} found in format string but no aur_command supplied",
            )?;
            Ok(Watched::Both(aur_command))
        } else if pacman && !aur {
            Ok(Watched::Pacman)
        } else if !pacman && aur {
            let aur_command = aur_command.block_error(
                "pacman",
                "{aur} found in format string but no aur_command supplied",
            )?;
            Ok(Watched::AUR(aur_command))
        } else {
            Ok(Watched::None)
        }
    }
}

impl ConfigBlock for Pacman {
    type Config = PacmanConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        shared_config: SharedConfig,
        _tx_update_request: Sender<Task>,
    ) -> Result<Self> {
        let output = TextWidget::new(id, 0, shared_config).with_icon("update")?;

        let fmt_normal = block_config.format.with_default("{pacman}")?;
        let fmt_singular = block_config.format_singular.with_default("{pacman}")?;
        let fmt_up_to_date = block_config.format_up_to_date.with_default("{pacman}")?;

        Ok(Pacman {
            id,
            update_interval: block_config.interval,
            output,
            warning_updates_regex: match block_config.warning_updates_regex {
                None => None, // no regex configured
                Some(regex_str) => {
                    let regex = Regex::new(regex_str.as_ref()).map_err(|_| {
                        ConfigurationError(
                            "pacman".to_string(),
                            "invalid warning updates regex".to_string(),
                        )
                    })?;
                    Some(regex)
                }
            },
            critical_updates_regex: match block_config.critical_updates_regex {
                None => None, // no regex configured
                Some(regex_str) => {
                    let regex = Regex::new(regex_str.as_ref()).map_err(|_| {
                        ConfigurationError(
                            "pacman".to_string(),
                            "invalid critical updates regex".to_string(),
                        )
                    })?;
                    Some(regex)
                }
            },
            watched: PacmanConfig::watched(
                &fmt_normal,
                &fmt_singular,
                &fmt_up_to_date,
                block_config.aur_command,
            )?,
            uptodate: false,
            hide_when_uptodate: block_config.hide_when_uptodate,
            format: fmt_normal,
            format_singular: fmt_singular,
            format_up_to_date: fmt_up_to_date,
        })
    }
}

fn has_fake_root() -> Result<bool> {
    has_command("pacman", "fakeroot")
}

fn check_fakeroot_command_exists() -> Result<()> {
    if !has_fake_root()? {
        Err(BlockError(
            "pacman".to_string(),
            "fakeroot not found".to_string(),
        ))
    } else {
        Ok(())
    }
}

fn get_updates_db_dir() -> Result<String> {
    let tmp_dir = env::temp_dir()
        .into_os_string()
        .into_string()
        .block_error("pacman", "There's something wrong with your $TMP variable")?;
    let user = env::var_os("USER")
        .unwrap_or_else(|| OsString::from(""))
        .into_string()
        .block_error("pacman", "There's a problem with your $USER")?;
    env::var_os("CHECKUPDATES_DB")
        .unwrap_or_else(|| OsString::from(format!("{}/checkup-db-{}", tmp_dir, user)))
        .into_string()
        .block_error("pacman", "There's a problem with your $CHECKUPDATES_DB")
}

fn get_pacman_available_updates() -> Result<String> {
    let updates_db = get_updates_db_dir()?;

    // Determine pacman database path
    let db_path = env::var_os("DBPath")
        .map(Into::into)
        .unwrap_or_else(|| Path::new("/var/lib/pacman/").to_path_buf());

    // Create the determined `checkup-db` path recursively
    fs::create_dir_all(&updates_db).block_error(
        "pacman",
        &format!("Failed to create checkup-db path '{}'", updates_db),
    )?;

    // Create symlink to local cache in `checkup-db` if required
    let local_cache = Path::new(&updates_db).join("local");
    if !local_cache.exists() {
        symlink(db_path.join("local"), local_cache)
            .block_error("pacman", "Failed to created required symlink")?;
    }

    // Update database
    Command::new("sh")
        .env("LC_ALL", "C")
        .args(&[
            "-c",
            &format!(
                "fakeroot -- pacman -Sy --dbpath \"{}\" --logfile /dev/null",
                updates_db
            ),
        ])
        .stdout(Stdio::null())
        .status()
        .block_error("pacman", "Failed to run command")?;

    // Get updates list
    String::from_utf8(
        Command::new("sh")
            .env("LC_ALL", "C")
            .args(&[
                "-c",
                &format!("fakeroot pacman -Qu --dbpath \"{}\"", updates_db),
            ])
            .output()
            .block_error("pacman", "There was a problem running the pacman commands")?
            .stdout,
    )
    .block_error(
        "pacman",
        "There was a problem while converting the output of the pacman command to a string",
    )
}

fn get_aur_available_updates(aur_command: &str) -> Result<String> {
    String::from_utf8(
        Command::new("sh")
            .args(&["-c", aur_command])
            .output()
            .block_error("pacman", &format!("aur command: {} failed", aur_command))?
            .stdout,
    )
    .block_error(
        "pacman",
        "There was a problem while converting the aur command output to a string",
    )
}

fn get_update_count(updates: &str) -> usize {
    updates
        .lines()
        .filter(|line| !line.contains("[ignored]"))
        .count()
}

fn has_warning_update(updates: &str, regex: &Regex) -> bool {
    updates.lines().filter(|line| regex.is_match(line)).count() > 0
}

fn has_critical_update(updates: &str, regex: &Regex) -> bool {
    updates.lines().filter(|line| regex.is_match(line)).count() > 0
}

impl Block for Pacman {
    fn id(&self) -> usize {
        self.id
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        if self.uptodate && self.hide_when_uptodate {
            vec![]
        } else {
            vec![&self.output]
        }
    }

    fn update(&mut self) -> Result<Option<Update>> {
        let (formatting_map, warning, critical, cum_count) = match &self.watched {
            Watched::Pacman => {
                check_fakeroot_command_exists()?;
                let pacman_available_updates = get_pacman_available_updates()?;
                let pacman_count = get_update_count(&pacman_available_updates);
                let formatting_map = map!(
                    "count" => Value::from_integer(pacman_count as i64),
                    "pacman" => Value::from_integer(pacman_count as i64),
                );

                let warning = self.warning_updates_regex.as_ref().map_or(false, |regex| {
                    has_warning_update(&pacman_available_updates, regex)
                });
                let critical = self.critical_updates_regex.as_ref().map_or(false, |regex| {
                    has_critical_update(&pacman_available_updates, regex)
                });

                (formatting_map, warning, critical, pacman_count)
            }
            Watched::AUR(aur_command) => {
                let aur_available_updates = get_aur_available_updates(aur_command)?;
                let aur_count = get_update_count(&aur_available_updates);
                let formatting_map = map!(
                    "aur" => Value::from_integer(aur_count as i64)
                );

                let warning = self.warning_updates_regex.as_ref().map_or(false, |regex| {
                    has_warning_update(&aur_available_updates, regex)
                });
                let critical = self.critical_updates_regex.as_ref().map_or(false, |regex| {
                    has_critical_update(&aur_available_updates, regex)
                });

                (formatting_map, warning, critical, aur_count)
            }
            Watched::Both(aur_command) => {
                check_fakeroot_command_exists()?;
                let pacman_available_updates = get_pacman_available_updates()?;
                let pacman_count = get_update_count(&pacman_available_updates);
                let aur_available_updates = get_aur_available_updates(aur_command)?;
                let aur_count = get_update_count(&aur_available_updates);
                let formatting_map = map!(
                    "count" =>  Value::from_integer(pacman_count as i64),
                    "pacman" => Value::from_integer(pacman_count as i64),
                    "aur" =>    Value::from_integer(aur_count as i64),
                    "both" =>   Value::from_integer((pacman_count + aur_count) as i64),
                );

                let warning = self.warning_updates_regex.as_ref().map_or(false, |regex| {
                    has_warning_update(&aur_available_updates, regex)
                        || has_warning_update(&pacman_available_updates, regex)
                });
                let critical = self.critical_updates_regex.as_ref().map_or(false, |regex| {
                    has_critical_update(&aur_available_updates, regex)
                        || has_critical_update(&pacman_available_updates, regex)
                });

                (formatting_map, warning, critical, pacman_count + aur_count)
            }
            Watched::None => (std::collections::HashMap::new(), false, false, 0),
        };
        self.output.set_texts(match cum_count {
            0 => self.format_up_to_date.render(&formatting_map)?,
            1 => self.format_singular.render(&formatting_map)?,
            _ => self.format.render(&formatting_map)?,
        });
        self.output.set_state(match cum_count {
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
        self.uptodate = cum_count == 0;
        Ok(Some(self.update_interval.into()))
    }

    fn click(&mut self, event: &I3BarEvent) -> Result<()> {
        if let MouseButton::Left = event.button {
            self.update()?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::blocks::pacman::{
        get_aur_available_updates, get_update_count, PacmanConfig, Watched,
    };
    use crate::formatting::FormatTemplate;

    #[test]
    fn test_get_update_count() {
        let no_update = "";
        assert_eq!(get_update_count(no_update), 0);
        let two_updates_available = concat!(
            "systemd 245.4-2 -> 245.5-1\n",
            "systemd-libs 245.4-2 -> 245.5-1\n"
        );
        assert_eq!(get_update_count(two_updates_available), 2);
    }

    #[test]
    fn test_watched() {
        let fmt_count = FormatTemplate::new("foo {count} bar", None).unwrap();
        let fmt_pacman = FormatTemplate::new("foo {pacman} bar", None).unwrap();
        let fmt_aur = FormatTemplate::new("foo {aur} bar", None).unwrap();
        let fmt_pacman_aur = FormatTemplate::new("foo {pacman} {aur} bar", None).unwrap();
        let fmt_both = FormatTemplate::new("foo {both} bar", None).unwrap();
        let fmt_none = FormatTemplate::new("foo bar", None).unwrap();
        let fmt_empty = FormatTemplate::new("", None).unwrap();

        let watched = PacmanConfig::watched(&fmt_count, &fmt_count, &fmt_empty, None);
        assert!(watched.is_ok());
        assert_eq!(watched.unwrap(), Watched::Pacman);
        let watched = PacmanConfig::watched(&fmt_pacman, &fmt_pacman, &fmt_empty, None);
        assert!(watched.is_ok());
        assert_eq!(watched.unwrap(), Watched::Pacman);
        let watched = PacmanConfig::watched(&fmt_none, &fmt_none, &fmt_empty, None);
        assert!(watched.is_ok()); // missing formatter should not cause an error
        let watched = PacmanConfig::watched(
            &fmt_none,
            &fmt_none,
            &fmt_empty,
            Some("aur cmd".to_string()),
        );
        assert!(watched.is_ok()); // missing formatter should not cause an error
        let watched =
            PacmanConfig::watched(&fmt_aur, &fmt_aur, &fmt_empty, Some("aur cmd".to_string()));
        assert!(watched.is_ok());
        assert_eq!(watched.unwrap(), Watched::AUR("aur cmd".to_string()));
        let watched = PacmanConfig::watched(
            &fmt_pacman_aur,
            &fmt_pacman_aur,
            &fmt_empty,
            Some("aur cmd".to_string()),
        );
        assert!(watched.is_ok());
        assert_eq!(watched.unwrap(), Watched::Both("aur cmd".to_string()));
        let watched = PacmanConfig::watched(&fmt_pacman_aur, &fmt_pacman_aur, &fmt_empty, None);
        assert!(watched.is_err()); // missing aur command
        let watched = PacmanConfig::watched(&fmt_both, &fmt_both, &fmt_empty, None);
        assert!(watched.is_err()); // missing aur command
        let watched = PacmanConfig::watched(
            &fmt_both,
            &fmt_both,
            &fmt_empty,
            Some("aur cmd".to_string()),
        );
        assert!(watched.is_ok());
        assert_eq!(watched.unwrap(), Watched::Both("aur cmd".to_string()));
    }

    #[test]
    fn test_get_aur_available_updates() {
        // aur_command should behave as echo -ne "foo x.x -> y.y\n"
        let updates = "foo x.x -> y.y\nbar x.x -> y.y\n";
        let aur_command = format!("printf '{}'", updates);
        let available_updates = get_aur_available_updates(&aur_command);
        assert!(available_updates.is_ok());
        assert_eq!(available_updates.unwrap(), updates);
    }
}

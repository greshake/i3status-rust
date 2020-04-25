use std::env;
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use crossbeam_channel::Sender;
use serde_derive::Deserialize;
use uuid::Uuid;

use crate::blocks::{Block, ConfigBlock};
use crate::config::Config;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::input::{I3BarEvent, MouseButton};
use crate::scheduler::Task;
use crate::util::{FormatTemplate, has_command};
use crate::widget::{I3BarWidget, State};
use crate::widgets::button::ButtonWidget;

pub struct Pacman {
    output: ButtonWidget,
    id: String,
    update_interval: Duration,
    format: FormatTemplate,
    format_singular: FormatTemplate,
    format_up_to_date: FormatTemplate,
    kernel_updates_are_critical: bool,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct PacmanConfig {
    /// Update interval in seconds
    #[serde(
        default = "PacmanConfig::default_interval",
        deserialize_with = "deserialize_duration"
    )]
    pub interval: Duration,

    /// Format override
    #[serde(default = "PacmanConfig::default_format")]
    pub format: String,

    /// Alternative format override for when exactly 1 update is available
    #[serde(default = "PacmanConfig::default_format")]
    pub format_singular: String,

    /// Alternative format override for when no updates are available
    #[serde(default = "PacmanConfig::default_format")]
    pub format_up_to_date: String,

    /// Indicate a `critical` state for the block if there are kernel updates available. Default
    /// behaviour is that kernel updates are treated as any other package update
    #[serde(default = "PacmanConfig::default_kernel_updates_are_critical")]
    pub kernel_updates_are_critical: bool,
}

impl PacmanConfig {
    fn default_interval() -> Duration {
        Duration::from_secs(60 * 10)
    }

    fn default_format() -> String {
        "{count}".to_owned()
    }

    fn default_kernel_updates_are_critical() -> bool {
        false
    }
}

impl ConfigBlock for Pacman {
    type Config = PacmanConfig;

    fn new(
        block_config: Self::Config,
        config: Config,
        _tx_update_request: Sender<Task>,
    ) -> Result<Self> {
        Ok(Pacman {
            id: Uuid::new_v4().to_simple().to_string(),
            update_interval: block_config.interval,
            format: FormatTemplate::from_string(&block_config.format)
                .block_error("pacman", "Invalid format specified for pacman::format")?,
            format_singular: FormatTemplate::from_string(&block_config.format_singular)
                .block_error(
                    "pacman",
                    "Invalid format specified for pacman::format_singular",
                )?,
            format_up_to_date: FormatTemplate::from_string(&block_config.format_up_to_date)
                .block_error(
                    "pacman",
                    "Invalid format specified for pacman::format_up_to_date",
                )?,
            output: ButtonWidget::new(config, "pacman").with_icon("update"),
            kernel_updates_are_critical: block_config.kernel_updates_are_critical,
        })
    }
}

fn run_command(var: &str) -> Result<()> {
    Command::new("sh")
        .args(&["-c", var])
        .spawn()
        .block_error("pacman", &format!("Failed to run command '{}'", var))?
        .wait()
        .block_error("pacman", &format!("Failed to wait for command '{}'", var))
        .map(|_| ())
}

fn has_fake_root() -> Result<bool> {
    has_command("pacman", "fakeroot")
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

fn get_updated_package_list_to_update() -> Result<String> {
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
    run_command(&format!(
        "fakeroot -- pacman -Sy --dbpath \"{}\" --logfile /dev/null &> /dev/null",
        updates_db
    ))?;

    // Get update count
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
        "There was an problem while converting the output of the pacman command to a string",
    )
}

fn get_update_count(list_of_packages: &String) -> Result<usize> {
    if !has_fake_root()? {
        return Ok(0 as usize);
    }
    // Get update count
    Ok(list_of_packages
        .lines()
        .filter(|line| !line.contains("[ignored]"))
        .count())
}

fn has_kernel_update(list_of_packages: &String) -> Result<bool> {
    // check if there are linux kernel updates
    Ok(list_of_packages
        .lines()
        .filter(|line| {
            line.starts_with("linux ")
                || line.starts_with("linux-hardened ")
                || line.starts_with("linux-lts ")
                || line.starts_with("linux-zen ")
        })
        .count()
        > 0)
}

impl Block for Pacman {
    fn update(&mut self) -> Result<Option<Duration>> {
        if !has_fake_root()? {
            return Err(BlockError(
                "pacman".to_string(),
                "fakeroot not found".to_string(),
            ));
        }
        let packages_to_update = get_updated_package_list_to_update()?;
        let count = get_update_count(&packages_to_update)?;
        let values = map!("{count}" => count);
        self.output.set_text(match count {
            0 => self.format_up_to_date.render_static_str(&values)?,
            1 => self.format_singular.render_static_str(&values)?,
            _ => self.format.render_static_str(&values)?,
        });
        self.output.set_state(match count {
            0 => State::Idle,
            _ => {
                if self.kernel_updates_are_critical && has_kernel_update(&packages_to_update)? {
                    State::Critical
                } else {
                    State::Info
                }
            }
        });
        Ok(Some(self.update_interval))
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.output]
    }

    fn id(&self) -> &str {
        &self.id
    }

    fn click(&mut self, event: &I3BarEvent) -> Result<()> {
        if event.name.as_ref().map(|s| s == "pacman").unwrap_or(false)
            && event.button == MouseButton::Left
        {
            self.update()?;
        }

        Ok(())
    }
}

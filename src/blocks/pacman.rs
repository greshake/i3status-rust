use crate::scheduler::Task;
use crossbeam_channel::Sender;
use serde_derive::Deserialize;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use crate::blocks::{Block, ConfigBlock};
use crate::config::Config;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::input::{I3BarEvent, MouseButton};
use crate::util::FormatTemplate;
use crate::widget::{I3BarWidget, State};
use crate::widgets::button::ButtonWidget;

use uuid::Uuid;

pub struct Pacman {
    output: ButtonWidget,
    id: String,
    update_interval: Duration,
    format: FormatTemplate,
    format_singular: FormatTemplate,
    format_up_to_date: FormatTemplate,
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
}

impl PacmanConfig {
    fn default_interval() -> Duration {
        Duration::from_secs(60 * 10)
    }

    fn default_format() -> String {
        "{count}".to_owned()
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
    Ok(String::from_utf8(
        Command::new("sh")
            .args(&["-c", "type -P fakeroot"])
            .output()
            .block_error("pacman", "failed to start command to check for fakeroot")?
            .stdout,
    )
    .block_error("pacman", "failed to check for fakeroot")?
    .trim()
        != "")
}

fn get_update_count() -> Result<usize> {
    if !has_fake_root()? {
        return Ok(0 as usize);
    }
    let tmp_dir = env::temp_dir()
        .into_os_string()
        .into_string()
        .block_error("pacman", "There's something wrong with your $TMP variable")?;
    let user = env::var_os("USER")
        .unwrap_or_else(|| OsString::from(""))
        .into_string()
        .block_error("pacman", "There's a problem with your $USER")?;
    let updates_db = env::var_os("CHECKUPDATES_DB")
        .unwrap_or_else(|| OsString::from(format!("{}/checkup-db-{}", tmp_dir, user)))
        .into_string()
        .block_error("pacman", "There's a problem with your $CHECKUPDATES_DB")?;

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
    Ok(String::from_utf8(
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
    .block_error("pacman", "there was a problem parsing the output")?
    .lines()
    .filter(|line| !line.contains("[ignored]"))
    .count())
}

impl Block for Pacman {
    fn update(&mut self) -> Result<Option<Duration>> {
        let count = get_update_count()?;
        let values = map!("{count}" => count);
        self.output.set_text(match count {
            0 => self.format_up_to_date.render_static_str(&values)?,
            1 => self.format_singular.render_static_str(&values)?,
            _ => self.format.render_static_str(&values)?,
        });
        self.output.set_state(match count {
            0 => State::Idle,
            _ => State::Info,
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

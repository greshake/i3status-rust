use std::fs;
use std::path::Path;
use std::os::unix::fs::symlink;
use std::time::Duration;
use std::process::Command;
use std::env;
use std::ffi::OsString;
use chan::Sender;
use scheduler::Task;

use block::{Block, ConfigBlock};
use config::Config;
use de::deserialize_duration;
use errors::*;
use input::{I3BarEvent, MouseButton};
use widgets::button::ButtonWidget;
use widget::{I3BarWidget, State};

use uuid::Uuid;

pub struct Pacman {
    output: ButtonWidget,
    id: String,
    update_interval: Duration,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct PacmanConfig {
    /// Update interval in seconds
    #[serde(default = "PacmanConfig::default_interval", deserialize_with = "deserialize_duration")]
    pub interval: Duration,
}

impl PacmanConfig {
    fn default_interval() -> Duration {
        Duration::from_secs(60 * 10)
    }
}

impl ConfigBlock for Pacman {
    type Config = PacmanConfig;

    fn new(block_config: Self::Config, config: Config, _tx_update_request: Sender<Task>) -> Result<Self> {
        Ok(Pacman {
            id: format!("{}", Uuid::new_v4().to_simple()),
            update_interval: block_config.interval,
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
    Ok(
        String::from_utf8(
            Command::new("sh")
                .args(&["-c", "type -P fakeroot"])
                .output()
                .block_error("pacman", "failed to start command to check for fakeroot")?
                .stdout,
        ).block_error("pacman", "failed to check for fakeroot")?
            .trim() != "",
    )
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
        .unwrap_or_else(|| {
            OsString::from(format!("{}/checkup-db-{}", tmp_dir, user))
        })
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
    Ok(
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
        ).block_error("pacman", "there was a problem parsing the output")?
            .lines()
            .filter(|line| !line.contains("[ignored]"))
            .count(),
    )
}


impl Block for Pacman {
    fn update(&mut self) -> Result<Option<Duration>> {
        let count = get_update_count()?;
        self.output.set_text(format!("{}", count));
        self.output.set_state(match count {
            0 => State::Idle,
            _ => State::Info,
        });
        Ok(Some(self.update_interval))

    }

    fn view(&self) -> Vec<&I3BarWidget> {
        vec![&self.output]
    }

    fn id(&self) -> &str {
        &self.id
    }

    fn click(&mut self, event: &I3BarEvent) -> Result<()> {
        if event.name.as_ref().map(|s| s == "pacman").unwrap_or(false) && event.button == MouseButton::Left {
            self.update()?;
        }

        Ok(())
    }
}

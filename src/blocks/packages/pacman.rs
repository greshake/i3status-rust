use std::env;
use std::path::PathBuf;
use std::process::Stdio;

use tokio::fs::{create_dir_all, symlink};
use tokio::process::Command;

use super::*;
use crate::util::has_command;

make_log_macro!(debug, "pacman");

static PACMAN_UPDATES_DB: Lazy<PathBuf> = Lazy::new(|| {
    let path = match env::var_os("CHECKUPDATES_DB") {
        Some(val) => val.into(),
        None => {
            let mut path = env::temp_dir();
            let user = env::var("USER");
            path.push(format!(
                "checkup-db-i3statusrs-{}",
                user.as_deref().unwrap_or("no-user")
            ));
            path
        }
    };
    debug!("Using {} as updates DB path", path.display());
    path
});

static PACMAN_DB: Lazy<PathBuf> = Lazy::new(|| {
    let path = env::var_os("DBPath")
        .map(Into::into)
        .unwrap_or_else(|| PathBuf::from("/var/lib/pacman/"));
    debug!("Using {} as pacman DB path", path.display());
    path
});

pub(super) struct Pacman;

pub(super) struct Aur {
    aur_command: String,
}

impl Pacman {
    pub(super) fn new() -> Self {
        Self
    }
}

impl Aur {
    pub(super) fn new() -> Self {
        Aur {
            aur_command: String::new(),
        }
    }
}

#[async_trait]
impl Backend for Pacman {
    async fn setup(&mut self) -> Result<()> {
        check_fakeroot_command_exists().await?;

        Ok(())
    }

    async fn get_updates_list(&self) -> Result<String> {
        // Create the determined `checkup-db` path recursively
        create_dir_all(&*PACMAN_UPDATES_DB).await.or_error(|| {
            format!(
                "Failed to create checkup-db directory at '{}'",
                PACMAN_UPDATES_DB.display()
            )
        })?;

        // Create symlink to local cache in `checkup-db` if required
        let local_cache = PACMAN_UPDATES_DB.join("local");
        if !local_cache.exists() {
            symlink(PACMAN_DB.join("local"), local_cache)
                .await
                .error("Failed to created required symlink")?;
        }

        // Update database
        let status = Command::new("fakeroot")
            .env("LC_ALL", "C")
            .args([
                "--".as_ref(),
                "pacman".as_ref(),
                "-Sy".as_ref(),
                "--dbpath".as_ref(),
                PACMAN_UPDATES_DB.as_os_str(),
                "--logfile".as_ref(),
                "/dev/null".as_ref(),
            ])
            .stdout(Stdio::null())
            .status()
            .await
            .error("Failed to run command")?;
        if !status.success() {
            debug!("{}", status);
            return Err(Error::new("pacman -Sy exited with non zero exit status"));
        }

        let stdout = Command::new("fakeroot")
            .env("LC_ALL", "C")
            .args([
                "--".as_ref(),
                "pacman".as_ref(),
                "-Qu".as_ref(),
                "--dbpath".as_ref(),
                PACMAN_UPDATES_DB.as_os_str(),
            ])
            .output()
            .await
            .error("There was a problem running the pacman commands")?
            .stdout;

        String::from_utf8(stdout).error("Pacman produced non-UTF8 output")
    }

    async fn get_update_count(&self, updates: &str) -> Result<usize> {
        Ok(updates
            .lines()
            .filter(|line| !line.contains("[ignored]"))
            .count())
    }
}

#[async_trait]
impl Backend for Aur {
    async fn setup(&mut self) -> Result<()> {
        // Nothing to setup here
        Ok(())
    }

    async fn get_updates_list(&self) -> Result<String> {
        let stdout = Command::new("sh")
            .args(["-c", &self.aur_command])
            .output()
            .await
            .or_error(|| format!("aur command: {} failed", self.aur_command))?
            .stdout;
        String::from_utf8(stdout)
            .error("There was a problem while converting the aur command output to a string")
    }

    async fn get_update_count(&self, updates: &str) -> Result<usize> {
        Ok(updates
            .lines()
            .filter(|line| !line.contains("[ignored]"))
            .count())
    }
}

async fn check_fakeroot_command_exists() -> Result<()> {
    if !has_command("fakeroot").await? {
        Err(Error::new("fakeroot not found"))
    } else {
        Ok(())
    }
}

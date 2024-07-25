use std::env;
use std::path::PathBuf;
use std::process::Stdio;

use tokio::fs::{create_dir_all, symlink};
use tokio::process::Command;

use super::*;
use crate::util::has_command;

make_log_macro!(debug, "pacman");

pub static PACMAN_UPDATES_DB: LazyLock<PathBuf> = LazyLock::new(|| {
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

pub static PACMAN_DB: LazyLock<PathBuf> = LazyLock::new(|| {
    let path = env::var_os("DBPath")
        .map(Into::into)
        .unwrap_or_else(|| PathBuf::from("/var/lib/pacman/"));
    debug!("Using {} as pacman DB path", path.display());
    path
});

pub struct Pacman;

pub struct Aur {
    aur_command: String,
}

impl Pacman {
    pub async fn new() -> Result<Self> {
        check_fakeroot_command_exists().await?;

        Ok(Self)
    }
}

impl Aur {
    pub fn new(aur_command: String) -> Self {
        Aur { aur_command }
    }
}

#[async_trait]
impl Backend for Pacman {
    fn name(&self) -> Cow<'static, str> {
        "pacman".into()
    }

    async fn get_updates_list(&self) -> Result<Vec<String>> {
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

        let updates = String::from_utf8(stdout).error("Pacman produced non-UTF8 output")?;

        let updates = updates
            .lines()
            .filter(|line| !line.contains("[ignored]"))
            .map(|line| line.to_string())
            .collect();

        Ok(updates)
    }
}

#[async_trait]
impl Backend for Aur {
    fn name(&self) -> Cow<'static, str> {
        "aur".into()
    }

    async fn get_updates_list(&self) -> Result<Vec<String>> {
        let stdout = Command::new("sh")
            .args(["-c", &self.aur_command])
            .output()
            .await
            .or_error(|| format!("aur command: {} failed", self.aur_command))?
            .stdout;
        let updates = String::from_utf8(stdout)
            .error("There was a problem while converting the aur command output to a string")?;

        let updates = updates
            .lines()
            .filter(|line| !line.contains("[ignored]"))
            .map(|line| line.to_string())
            .collect();

        Ok(updates)
    }
}

async fn check_fakeroot_command_exists() -> Result<()> {
    if !has_command("fakeroot").await? {
        Err(Error::new("fakeroot not found"))
    } else {
        Ok(())
    }
}

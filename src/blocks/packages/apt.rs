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
//! `ignore_updates_regex` | Doesn't include updates matching regex in the count. | `None`
//! `ignore_phased_updates` | Doesn't include potentially held back phased updates in the count. | `false`
//!
//! Placeholder | Value                       | Type   | Unit
//! ------------|-----------------------------|--------|------
//! `icon`      | A static icon               | Icon   | -
//! `apt`       | Number of updates available | Number | -
//!
//! # Example
//!
//! Update the list of pending updates every thirty minutes (1800 seconds):
//!
//! ```toml
//! [[block]]
//! block = "packages"
//! interval = 1800
//! format = " $icon $apt updates available"
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

use tokio::fs::{create_dir_all, File};
use tokio::process::Command;

use super::*;

pub(super) struct Apt {
    pub(super) config_file: String,
    pub(super) ignore_phased_updates: bool,
    pub(super) ignore_updates_regex: Option<Regex>,
}

impl Apt {
    pub(super) fn new() -> Self {
        Apt {
            config_file: String::new(),
            ignore_phased_updates: false,
            ignore_updates_regex: Default::default(),
        }
    }
}

#[async_trait]
impl Backend for Apt {
    async fn setup(&mut self) -> Result<()> {
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
        let config_file = config_file.to_str().unwrap();

        self.config_file = config_file.to_string();

        let mut file = File::create(&config_file)
            .await
            .error("Failed to create config file")?;
        file.write_all(apt_config.as_bytes())
            .await
            .error("Failed to write to config file")?;

        Ok(())
    }

    async fn get_updates_list(&self) -> Result<String> {
        Command::new("apt")
            .env("APT_CONFIG", &self.config_file)
            .args(["update"])
            .stdout(Stdio::null())
            .stdin(Stdio::null())
            .spawn()
            .error("Failed to run `apt update`")?
            .wait()
            .await
            .error("Failed to run `apt update`")?;
        let stdout = Command::new("apt")
            .env("LANG", "C")
            .env("APT_CONFIG", &self.config_file)
            .args(["list", "--upgradable"])
            .output()
            .await
            .error("Problem running apt command")?
            .stdout;
        String::from_utf8(stdout).error("apt produced non-UTF8 output")
    }

    async fn get_update_count(&self, updates: &str) -> Result<usize> {
        let mut cnt = 0;

        for update_line in updates
            .lines()
            .filter(|line| line.contains("[upgradable"))
            .filter(|line| {
                self.ignore_updates_regex
                    .as_ref()
                    .map_or(true, |re| !re.is_match(line))
            })
        {
            if !self.ignore_phased_updates
                || !is_phased_update(&self.config_file, update_line).await?
            {
                cnt += 1;
            }
        }

        Ok(cnt)
    }
}

async fn is_phased_update(config_path: &str, package_line: &str) -> Result<bool> {
    let package_name_regex = regex!(r#"(.*)/.*"#);
    let package_name = &package_name_regex
        .captures(package_line)
        .error("Couldn't find package name")?[1];

    let output = String::from_utf8(
        Command::new("apt-cache")
            .args(["-c", config_path, "policy", package_name])
            .output()
            .await
            .error("Problem running apt-cache command")?
            .stdout,
    )
    .error("Problem capturing apt-cache command output")?;

    let phased_regex = regex!(r".*\(phased (\d+)%\).*");
    Ok(match phased_regex.captures(&output) {
        Some(matches) => &matches[1] != "100",
        None => false,
    })
}

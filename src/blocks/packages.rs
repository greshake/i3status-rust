//! Pending updates for different package manager like apt, pacman, etc.
//!
//! Currently, these package managers are supported:
//! - `apk` for Alpine Linux
//! - `apt` for Debian/Ubuntu-based systems
//! - `aur` for Arch-based systems
//! - `brew` for the Homebrew Package Manager
//! - `dnf` for Fedora-based systems
//! - `flatpak` for Flatpak packages
//! - `pacman` for Arch-based systems
//! - `snap` for Snap packages
//! - `xbps` for Void Linux
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `interval` | Update interval in seconds. | `600`
//! `package_manager` | Package manager to check for updates | Automatically derived from format templates, but can be used to influence the `$total` value
//! `format` | A string to customise the output of this block. See below for available placeholders. | `" $icon $total.eng(w:1) "`
//! `format_singular` | Same as `format`, but for when exactly one update is available. | `" $icon $total.eng(w:1) "`
//! `format_up_to_date` | Same as `format`, but for when no updates are available. | `" $icon $total.eng(w:1) "`
//! `warning_updates_regex` | Display block as warning if updates matching regex are available. | `None`
//! `critical_updates_regex` | Display block as critical if updates matching regex are available. | `None`
//! `ignore_updates_regex` | Doesn't include updates matching regex in the count. | `None`
//! `ignore_phased_updates` | Doesn't include potentially held back phased updates in the count. (For Debian/Ubuntu-based systems) | `false`
//! `aur_command` | AUR command to check available updates, which outputs in the same format as pacman. E.g. `yay -Qua` (For Arch-based systems) | Required if `$aur` is used
//!
//!  Placeholder | Value                                                                            | Type   | Unit
//! -------------|----------------------------------------------------------------------------------|--------|-----
//! `icon`       | A static icon                                                                    | Icon   | -
//! `apk`        | Number of updates available in Alpine Linux                                      | Number | -
//! `apt`        | Number of updates available in Debian/Ubuntu-based systems                       | Number | -
//! `aur`        | Number of updates available in Arch-based systems                                | Number | -
//! `brew`       | Number of updates available in the Homebrew Package Manager                      | Number | -
//! `dnf`        | Number of updates available in Fedora-based systems                              | Number | -
//! `flatpak`    | Number of updates available in Flatpak packages                                  | Number | -
//! `pacman`     | Number of updates available in Arch-based systems                                | Number | -
//! `snap`       | Number of updates available in Snap packages                                     | Number | -
//! `xbps`       | Number of updates available in Void Linux                                        | Number | -
//! `total`      | Number of updates available in all package manager listed                        | Number | -
//!
//! # Apt
//!
//! Behind the scenes this uses `apt`, and in order to run it without root privileges i3status-rust will create its own package database in `/tmp/i3rs-apt/` which may take up several MB or more. If you have a custom apt config then this block may not work as expected - in that case please open an issue.
//!
//! Tip: You can grab the list of available updates using `APT_CONFIG=/tmp/i3rs-apt/apt.conf apt list --upgradable`
//!
//! # Pacman
//!
//! Requires fakeroot to be installed (only required for pacman).
//!
//! Tip: You can grab the list of available updates using `fakeroot pacman -Qu --dbpath /tmp/checkup-db-i3statusrs-$USER/`.
//! If you have the `CHECKUPDATES_DB` env var set on your system then substitute that dir instead.
//!
//! Note: `pikaur` may hang the whole block if there is no internet connectivity [reference](https://github.com/actionless/pikaur/issues/595). In that case, try a different AUR helper.
//!
//! ### Pacman hook
//!
//! Tip: On Arch Linux you can setup a `pacman` hook to signal i3status-rs to update after packages
//! have been upgraded, so you won't have stale info in your pacman block.
//!
//! In the block configuration, set `signal = 1` (or another number if `1` is being used by some
//! other block):
//!
//! ```toml
//! [[block]]
//! block = "packages"
//! signal = 1
//! ```
//!
//! Create `/etc/pacman.d/hooks/i3status-rust.hook` with the below contents:
//!
//! ```ini
//! [Trigger]
//! Operation = Upgrade
//! Type = Package
//! Target = *
//!
//! [Action]
//! When = PostTransaction
//! Exec = /usr/bin/pkill -SIGRTMIN+1 i3status-rs
//! ```
//!
//! # Example
//!
//! Apk-only config:
//!
//! ```toml
//! [[block]]
//! block = "packages"
//! package_manager = ["apk"]
//! interval = 1800
//! error_interval = 300
//! max_retries = 5
//! format = " $icon $apk.eng(w:1) updates available "
//! format_singular = " $icon One update available "
//! format_up_to_date = " $icon system up to date "
//! [[block.click]]
//! # shows dmenu with available updates. Any dmenu alternative should also work.
//! button = "left"
//! cmd = "apk --no-cache --upgradable list | dmenu -l 10"
//! ```
//!
//! Apt-only config
//!
//! ```toml
//! [[block]]
//! block = "packages"
//! interval = 1800
//! error_interval = 300
//! max_retries = 5
//! package_manager = ["apt"]
//! format = " $icon $apt updates available"
//! format_singular = " $icon One update available "
//! format_up_to_date = " $icon system up to date "
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
//! Brew-only config:
//!
//! ```toml
//! [[block]]
//! block = "packages"
//! package_manager = ["brew"]
//! interval = 1800
//! error_interval = 300
//! max_retries = 5
//! format = " $icon $brew.eng(w:1) updates available "
//! format_singular = " $icon One update available "
//! format_up_to_date = " $icon system up to date "
//! [[block.click]]
//! # shows dmenu with available updates. Any dmenu alternative should also work.
//! button = "left"
//! cmd = "brew outdated | dmenu -l 10"
//! ```
//!
//! Dnf-only config:
//!
//! ```toml
//! [[block]]
//! block = "packages"
//! package_manager = ["dnf"]
//! interval = 1800
//! error_interval = 300
//! max_retries = 5
//! format = " $icon $dnf.eng(w:1) updates available "
//! format_singular = " $icon One update available "
//! format_up_to_date = " $icon system up to date "
//! [[block.click]]
//! # shows dmenu with cached available updates. Any dmenu alternative should also work.
//! button = "left"
//! cmd = "dnf list -q --upgrades | tail -n +2 | rofi -dmenu"
//! ```
//!
//! Flatpak-only config:
//!
//! ```toml
//! [[block]]
//! block = "packages"
//! package_manager = ["flatpak"]
//! interval = 1800
//! error_interval = 300
//! max_retries = 5
//! format = " $icon $flatpak.eng(w:1) updates available "
//! format_singular = " $icon One update available "
//! format_up_to_date = " $icon system up to date "
//! [[block.click]]
//! # shows dmenu with cached available updates. Any dmenu alternative should also work.
//! button = "left"
//! cmd = "flatpak remote-ls --updates --no-header --columns=ref | tail -n +2 | rofi -dmenu"
//! ```
//!
//! Pacman-only config:
//!
//! ```toml
//! [[block]]
//! block = "packages"
//! package_manager = ["pacman"]
//! interval = 600
//! error_interval = 300
//! max_retries = 5
//! format = " $icon $pacman updates available "
//! format_singular = " $icon $pacman update available "
//! format_up_to_date = " $icon system up to date "
//! [[block.click]]
//! # pop-up a menu showing the available updates. Replace wofi with your favourite menu command.
//! button = "left"
//! cmd = "fakeroot pacman -Qu --dbpath /tmp/checkup-db-i3statusrs-$USER/ | wofi --show dmenu"
//! [[block.click]]
//! # Updates the block on right click
//! button = "right"
//! update = true
//! ```
//!
//! Pacman and AUR helper config:
//!
//! ```toml
//! [[block]]
//! block = "packages"
//! package_manager = ["pacman", "aur"]
//! interval = 600
//! error_interval = 300
//! max_retries = 5
//! format = " $icon $pacman + $aur = $total updates available "
//! format_singular = " $icon $total update available "
//! format_up_to_date = " $icon system up to date "
//! # aur_command should output available updates to stdout (ie behave as echo -ne "update\n")
//! aur_command = "yay -Qua"
//! ```
//!
//! Snap-only config:
//!
//! ```toml
//! [[block]]
//! block = "packages"
//! package_manager = ["snap"]
//! interval = 1800
//! error_interval = 300
//! max_retries = 5
//! format = " $icon $snap.eng(w:1) updates available "
//! format_singular = " $icon One update available "
//! format_up_to_date = " $icon system up to date "
//! [[block.click]]
//! # shows dmenu with available updates. Any dmenu alternative should also work.
//! button = "left"
//! cmd = "snap refresh --list | dmenu -l 10"
//! ```
//!
//! Xbps-only config:
//!
//! ```toml
//! [[block]]
//! block = "packages"
//! package_manager = ["xbps"]
//! interval = 1800
//! error_interval = 300
//! max_retries = 5
//! format = " $icon $xbps.eng(w:1) updates available "
//! format_singular = " $icon One update available "
//! format_up_to_date = " $icon system up to date "
//! [[block.click]]
//! # shows dmenu with available updates. Any dmenu alternative should also work.
//! button = "left"
//! cmd = "xbps-install -Mun | dmenu -l 10"
//! ```
//!
//! Multiple package managers config:
//!
//! Update the list of pending updates every thirty minutes (1800 seconds):
//!
//! ```toml
//! [[block]]
//! block = "packages"
//! package_manager = ["apk", "apt", "aur", "brew", "dnf", "flatpak", "pacman", "snap", "xbps"]
//! interval = 1800
//! error_interval = 300
//! max_retries = 5
//! format = " $icon $apk + $apt + $aur + $brew + $dnf + $flatpak + $pacman + $snap + $xbps = $total updates available "
//! format_singular = " $icon One update available "
//! format_up_to_date = " $icon system up to date "
//! # If a linux update is available, but no ZFS package, it won't be possible to
//! # actually perform a system upgrade, so we show a warning.
//! warning_updates_regex = "(linux|linux-lts|linux-zen)"
//! # If ZFS is available, we know that we can and should do an upgrade, so we show
//! # the status as critical.
//! critical_updates_regex = "(zfs|zfs-lts)"
//! ```
//!
//! # Icons Used
//!
//! - `update`

pub mod apk;
use apk::Apk;

pub mod apt;
use apt::Apt;

pub mod brew;
use brew::Brew;

pub mod dnf;
use dnf::Dnf;

pub mod flatpak;
use flatpak::Flatpak;

pub mod pacman;
use pacman::{Aur, Pacman};

pub mod xbps;
use xbps::Xbps;

pub mod snap;
use snap::Snap;

use regex::Regex;

use super::prelude::*;

#[derive(Deserialize, Debug, SmartDefault, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    #[default(600.into())]
    pub interval: Seconds,
    pub package_manager: Vec<PackageManager>,
    pub format: FormatConfig,
    pub format_singular: FormatConfig,
    pub format_up_to_date: FormatConfig,
    pub warning_updates_regex: Option<String>,
    pub critical_updates_regex: Option<String>,
    pub ignore_updates_regex: Option<String>,
    pub ignore_phased_updates: bool,
    pub aur_command: Option<String>,
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PackageManager {
    Apk,
    Apt,
    Aur,
    Brew,
    Dnf,
    Flatpak,
    Pacman,
    Snap,
    Xbps,
}

impl PackageManager {
    /// The name of the package manager, as used in format strings.
    fn name(&self) -> &'static str {
        match self {
            PackageManager::Apk => "apk",
            PackageManager::Apt => "apt",
            PackageManager::Aur => "aur",
            PackageManager::Brew => "brew",
            PackageManager::Dnf => "dnf",
            PackageManager::Flatpak => "flatpak",
            PackageManager::Pacman => "pacman",
            PackageManager::Snap => "snap",
            PackageManager::Xbps => "xbps",
        }
    }

    /// Builds a backend for the package manager.
    async fn build(&self, config: &Config) -> Result<Box<dyn Backend>> {
        Ok(match self {
            PackageManager::Apk => Box::new(Apk::new()),
            PackageManager::Apt => Box::new(Apt::new(config.ignore_phased_updates).await?),
            PackageManager::Aur => Box::new(Aur::new(
                config.aur_command.clone().error("aur_command is not set")?,
            )),
            PackageManager::Brew => Box::new(Brew::new()),
            PackageManager::Dnf => Box::new(Dnf::new()),
            PackageManager::Flatpak => Box::new(Flatpak::new()),
            PackageManager::Pacman => Box::new(Pacman::new().await?),
            PackageManager::Snap => Box::new(Snap::new()),
            PackageManager::Xbps => Box::new(Xbps::new()),
        })
    }
}

pub async fn run(config: &Config, api: &CommonApi) -> Result<()> {
    let mut config: Config = config.clone();

    let format = config.format.with_default(" $icon $total.eng(w:1) ")?;
    let format_singular = config
        .format_singular
        .with_default(" $icon $total.eng(w:1) ")?;
    let format_up_to_date = config
        .format_up_to_date
        .with_default(" $icon $total.eng(w:1) ")?;

    // Check if the user specified a package manager in any format string, then
    // add that package manager to the config list.
    macro_rules! check_manager {
        ($manager:expr) => {{
            let name = $manager.name();
            let in_format = format.contains_key(name)
                || format_singular.contains_key(name)
                || format_up_to_date.contains_key(name);

            if !config.package_manager.contains(&$manager) && in_format {
                config.package_manager.push($manager);
            }
        }};
    }

    check_manager!(PackageManager::Apk);
    check_manager!(PackageManager::Apt);
    check_manager!(PackageManager::Aur);
    check_manager!(PackageManager::Brew);
    check_manager!(PackageManager::Dnf);
    check_manager!(PackageManager::Flatpak);
    check_manager!(PackageManager::Pacman);
    check_manager!(PackageManager::Snap);
    check_manager!(PackageManager::Xbps);

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
    let ignore_updates_regex = config
        .ignore_updates_regex
        .as_deref()
        .map(Regex::new)
        .transpose()
        .error("invalid ignore updates regex")?;

    let mut package_manager_vec: Vec<Box<dyn Backend>> = Vec::new();

    for &package_manager in config.package_manager.iter() {
        package_manager_vec.push(package_manager.build(&config).await?);
    }

    loop {
        let mut package_manager_map: HashMap<Cow<'static, str>, Value> = HashMap::new();

        let mut critical = false;
        let mut warning = false;
        let mut total_count = 0;

        // Iterate over the all package manager listed in Config
        for package_manager in &package_manager_vec {
            let mut updates = package_manager.get_updates_list().await?;
            if let Some(regex) = ignore_updates_regex.clone() {
                updates.retain(|u| !regex.is_match(u));
            }

            let updates_count = updates.len();

            package_manager_map.insert(package_manager.name(), Value::number(updates_count));
            total_count += updates_count;

            warning |= warning_updates_regex
                .as_ref()
                .is_some_and(|regex| has_matching_update(&updates, regex));
            critical |= critical_updates_regex
                .as_ref()
                .is_some_and(|regex| has_matching_update(&updates, regex));
        }

        let mut widget = Widget::new();

        package_manager_map.insert("icon".into(), Value::icon("update"));
        package_manager_map.insert("total".into(), Value::number(total_count));

        widget.set_format(match total_count {
            0 => format_up_to_date.clone(),
            1 => format_singular.clone(),
            _ => format.clone(),
        });
        widget.set_values(package_manager_map);

        widget.state = match total_count {
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
        api.set_widget(widget)?;

        select! {
            _ = sleep(config.interval.0) => (),
            _ = api.wait_for_update_request() => (),
        }
    }
}

#[async_trait]
pub trait Backend {
    fn name(&self) -> Cow<'static, str>;

    async fn get_updates_list(&self) -> Result<Vec<String>>;
}

pub fn has_matching_update(updates: &[String], regex: &Regex) -> bool {
    updates.iter().any(|line| regex.is_match(line))
}

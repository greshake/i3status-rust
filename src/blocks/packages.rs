//! Shows pending updates for different package manager like apt, pacman, etc.
//!
//! Currently 2 package manager are available:
//! - `apt` for Debian/Ubuntu based system
//! - `pacman` for Arch based system
//! - `aur` for Arch based system
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `interval` | Update interval in seconds. | `600`
//! `package_manager` | Package manager to check for updates | -
//! `format` | A string to customise the output of this block. See below for available placeholders. | `" $icon $count.eng(w:1) "`
//! `format_singular` | Same as `format`, but for when exactly one update is available. | `" $icon $count.eng(w:1) "`
//! `format_up_to_date` | Same as `format`, but for when no updates are available. | `" $icon $count.eng(w:1) "`
//! `warning_updates_regex` | Display block as warning if updates matching regex are available. | `None`
//! `critical_updates_regex` | Display block as critical if updates matching regex are available. | `None`
//! `ignore_updates_regex` | Doesn't include updates matching regex in the count. | `None`
//! `ignore_phased_updates` | Doesn't include potentially held back phased updates in the count. (For Debian/Ubuntu based system) | `false`
//! `aur_command` | AUR command to check available updates, which outputs in the same format as pacman. e.g. `yay -Qua` (For Arch based system) | Required if `$both` or `$aur` are used
//!
//!  Placeholder | Value                                                                            | Type   | Unit
//! -------------|----------------------------------------------------------------------------------|--------|-----
//! `icon`       | A static icon                                                                    | Icon   | -
//! `apt`        | Number of updates available in Debian/Ubuntu based system                        | Number | -
//! `pacman`     | Number of updates available in Arch based system                                 | Number | -
//! `aur`        | Number of updates available in Arch based system                                 | Number | -
//! `total`      | Number of updates available in all package manager listed                        | Number | -
//!
//! # Example
//!
//! Update the list of pending updates every thirty minutes (1800 seconds):
//!
//! ```toml
//! [[block]]
//! block = "packages"
//! package_manager = ["apt", "pacman", "aur"]
//! interval = 1800
//! format = " $icon $apt + $pacman + $aur = $total updates available "
//! format_singular = " $icon One update available "
//! format_up_to_date = " $icon system up to date "
//! ```
//!
//! # Icons Used
//!
//! - `update`

pub mod apt;
use apt::Apt;

pub mod pacman;
use pacman::{Aur, Pacman};

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
    Apt,
    Pacman,
    Aur,
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

    // If user provide package manager in any of the formats then consider that also
    macro_rules! any_format_contains {
        ($name:expr) => {
            format.contains_key($name)
                || format_singular.contains_key($name)
                || format_up_to_date.contains_key($name)
        };
    }

    let apt = any_format_contains!("apt");
    let aur = any_format_contains!("aur");
    let pacman = any_format_contains!("pacman");

    if !config.package_manager.contains(&PackageManager::Apt) && apt {
        config.package_manager.push(PackageManager::Apt);
    }
    if !config.package_manager.contains(&PackageManager::Pacman) && pacman {
        config.package_manager.push(PackageManager::Pacman);
    }
    if !config.package_manager.contains(&PackageManager::Aur) && aur {
        config.package_manager.push(PackageManager::Aur);
    }

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

    // Setup once everything it takes to check updates for every package manager
    for package_manager in &config.package_manager {
        let mut backend: Box<dyn Backend> = match package_manager {
            PackageManager::Apt => Box::new(Apt::new()),
            PackageManager::Pacman => Box::new(Pacman::new()),
            PackageManager::Aur => Box::new(Aur::new()),
        };

        backend.setup().await?;
    }

    loop {
        let (mut apt_count, mut pacman_count, mut aur_count) = (0, 0, 0);
        let mut critical_vec = vec![false];
        let mut warning_vec = vec![false];

        // Iterate over the all package manager listed in Config
        for package_manager in config.package_manager.clone() {
            match package_manager {
                PackageManager::Apt => {
                    let mut apt = Apt::new();
                    apt.ignore_updates_regex = ignore_updates_regex.clone();
                    apt.ignore_phased_updates = config.ignore_phased_updates;
                    let updates = apt.get_updates_list().await?;
                    apt_count = apt.get_update_count(&updates).await?;
                    let warning = warning_updates_regex
                        .as_ref()
                        .is_some_and(|regex| apt.has_matching_update(&updates, regex));
                    let critical = critical_updates_regex
                        .as_ref()
                        .is_some_and(|regex| apt.has_matching_update(&updates, regex));
                    warning_vec.push(warning);
                    critical_vec.push(critical);
                }
                PackageManager::Pacman => {
                    let pacman = Pacman::new();
                    let updates = pacman.get_updates_list().await?;
                    pacman_count = pacman.get_update_count(&updates).await?;
                    let warning = warning_updates_regex
                        .as_ref()
                        .is_some_and(|regex| pacman.has_matching_update(&updates, regex));
                    let critical = critical_updates_regex
                        .as_ref()
                        .is_some_and(|regex| pacman.has_matching_update(&updates, regex));
                    warning_vec.push(warning);
                    critical_vec.push(critical);
                }
                PackageManager::Aur => {
                    let aur = Aur::new();
                    let updates = aur.get_updates_list().await?;
                    aur_count = aur.get_update_count(&updates).await?;
                    let warning = warning_updates_regex
                        .as_ref()
                        .is_some_and(|regex| aur.has_matching_update(&updates, regex));
                    let critical = critical_updates_regex
                        .as_ref()
                        .is_some_and(|regex| aur.has_matching_update(&updates, regex));
                    warning_vec.push(warning);
                    critical_vec.push(critical);
                }
            }
        }

        let mut widget = Widget::new();

        let total_count = apt_count + pacman_count + aur_count;
        widget.set_format(match total_count {
            0 => format_up_to_date.clone(),
            1 => format_singular.clone(),
            _ => format.clone(),
        });
        widget.set_values(map!(
            "icon" => Value::icon("update"),
            "apt" => Value::number(apt_count),
            "pacman" => Value::number(pacman_count),
            "aur" => Value::number(aur_count),
            "total" => Value::number(total_count),
        ));

        let warning = warning_vec.iter().any(|&x| x);
        let critical = critical_vec.iter().any(|&x| x);

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
trait Backend {
    async fn setup(&mut self) -> Result<()>;

    async fn get_updates_list(&self) -> Result<String>;

    async fn get_update_count(&self, updates: &str) -> Result<usize>;

    fn has_matching_update(&self, updates: &str, regex: &Regex) -> bool {
        updates.lines().any(|line| regex.is_match(line))
    }
}

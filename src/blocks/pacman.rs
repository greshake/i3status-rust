//! Pending updates available on pacman or an AUR helper.
//!
//! Requires fakeroot to be installed (only required for pacman).
//!
//! Tip: You can grab the list of available updates using `fakeroot pacman -Qu --dbpath /tmp/checkup-db-i3statusrs-$USER/`.
//! If you have the `CHECKUPDATES_DB` env var set on your system then substitute that dir instead.
//!
//! Note: `pikaur` may hang the whole block if there is no internet connectivity [reference](https://github.com/actionless/pikaur/issues/595). In that case, try a different AUR helper.
//!
//! # Pacman hook
//!
//! Tip: On Arch Linux you can setup a `pacman` hook to signal i3status-rs to update after packages
//! have been upgraded, so you won't have stale info in your pacman block.
//!
//! In the block configuration, set `signal = 1` (or other number if `1` is being used by some
//! other block):
//!
//! ```toml
//! [[block]]
//! block = "pacman"
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
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|---------
//! `interval` | Update interval, in seconds. If setting `aur_command` then set interval appropriately as to not exceed the AUR's daily rate limit. | `600`
//! `format` | A string to customise the output of this block. See below for available placeholders. | `" $icon $pacman.eng(w:1) "`
//! `format_singular` | Same as `format` but for when exactly one update is available. | `" $icon $pacman.eng(w:1) "`
//! `format_up_to_date` | Same as `format` but for when no updates are available. | `" $icon $pacman.eng(w:1) "`
//! `warning_updates_regex` | Display block as warning if updates matching regex are available. | `None`
//! `critical_updates_regex` | Display block as critical if updates matching regex are available. | `None`
//! `aur_command` | AUR command to check available updates, which outputs in the same format as pacman. e.g. `yay -Qua` | Required if `$both` or `$aur` are used
//!
//!  Placeholder | Value | Type | Unit
//! -------------|----------------------------------------------------------------------------------|--------|-----
//! `icon`       | A static icon                                                                    | Icon   | -
//! `pacman`     | Number of updates available according to `pacman`                                | Number | -
//! `aur`        | Number of updates available according to `<aur_command>`                         | Number | -
//! `both`       | Cumulative number of updates available according to `pacman` and `<aur_command>` | Number | -
//!
//! # Examples
//!
//! pacman only config:
//!
//! ```toml
//! [[block]]
//! block = "pacman"
//! interval = 600
//! format = " $icon $pacman updates available "
//! format_singular = " $icon $pacman update available "
//! format_up_to_date = " $icon system up to date "
//! critical_updates_regex = "(linux|linux-lts|linux-zen)"
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
//! pacman only config using warnings with ZFS modules:
//!
//! ```toml
//! [[block]]
//! block = "pacman"
//! interval = 600
//! format = " $icon $pacman updates available "
//! format_singular = " $icon $pacman update available "
//! format_up_to_date = " $icon system up to date "
//! # If a linux update is available, but no ZFS package, it won't be possible to
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
//! error_interval = 300
//! format = " $icon $pacman + $aur = $both updates available "
//! format_singular = " $icon $both update available "
//! format_up_to_date = " $icon system up to date "
//! critical_updates_regex = "(linux|linux-lts|linux-zen)"
//! # aur_command should output available updates to stdout (ie behave as echo -ne "update\n")
//! aur_command = "yay -Qua"
//! ```
//!
//! # Icons Used
//!
//! - `update`

use regex::Regex;

use super::{
    packages::{
        has_matching_update,
        pacman::{Aur, Pacman},
        Backend,
    },
    prelude::*,
};

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    #[default(600.into())]
    pub interval: Seconds,
    pub format: FormatConfig,
    pub format_singular: FormatConfig,
    pub format_up_to_date: FormatConfig,
    pub warning_updates_regex: Option<String>,
    pub critical_updates_regex: Option<String>,
    pub aur_command: Option<String>,
}

pub async fn run(config: &Config, api: &CommonApi) -> Result<()> {
    let format = config.format.with_default(" $icon $pacman.eng(w:1) ")?;
    let format_singular = config
        .format_singular
        .with_default(" $icon $pacman.eng(w:1) ")?;
    let format_up_to_date = config
        .format_up_to_date
        .with_default(" $icon $pacman.eng(w:1) ")?;

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
                .as_deref()
                .error("$aur or $both found in format string but no aur_command supplied")?,
        )
    } else if pacman && !aur {
        Watched::Pacman
    } else if !pacman && aur {
        Watched::Aur(
            config
                .aur_command
                .as_deref()
                .error("$aur or $both found in format string but no aur_command supplied")?,
        )
    } else {
        Watched::None
    };

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

    let pacman_backend = Pacman::new().await?;
    let aur_backend = Aur::new(config.aur_command.clone().unwrap_or_default());

    loop {
        let (mut values, warning, critical, total) = match &watched {
            Watched::Pacman => {
                let updates = pacman_backend.get_updates_list().await?;
                let count = updates.len();
                let values = map!("pacman" => Value::number(count));
                let warning = warning_updates_regex
                    .as_ref()
                    .is_some_and(|regex| has_matching_update(&updates, regex));
                let critical = critical_updates_regex
                    .as_ref()
                    .is_some_and(|regex| has_matching_update(&updates, regex));
                (values, warning, critical, count)
            }
            Watched::Aur(_) => {
                let updates = aur_backend.get_updates_list().await?;
                let count = updates.len();
                let values = map!(
                    "aur" => Value::number(count)
                );
                let warning = warning_updates_regex
                    .as_ref()
                    .is_some_and(|regex| has_matching_update(&updates, regex));
                let critical = critical_updates_regex
                    .as_ref()
                    .is_some_and(|regex| has_matching_update(&updates, regex));
                (values, warning, critical, count)
            }
            Watched::Both(_) => {
                let (pacman_updates, aur_updates) = tokio::try_join!(
                    pacman_backend.get_updates_list(),
                    aur_backend.get_updates_list(),
                )?;
                let pacman_count = pacman_updates.len();
                let aur_count = aur_updates.len();
                let values = map! {
                    "pacman" => Value::number(pacman_count),
                    "aur" =>    Value::number(aur_count),
                    "both" =>   Value::number(pacman_count + aur_count),
                };
                let warning = warning_updates_regex.as_ref().is_some_and(|regex| {
                    has_matching_update(&aur_updates, regex)
                        || has_matching_update(&pacman_updates, regex)
                });
                let critical = critical_updates_regex.as_ref().is_some_and(|regex| {
                    has_matching_update(&aur_updates, regex)
                        || has_matching_update(&pacman_updates, regex)
                });
                (values, warning, critical, pacman_count + aur_count)
            }
            Watched::None => (HashMap::new(), false, false, 0),
        };
        values.insert("icon".into(), Value::icon("update"));

        let mut widget = Widget::new();
        widget.set_format(match total {
            0 => format_up_to_date.clone(),
            1 => format_singular.clone(),
            _ => format.clone(),
        });
        widget.set_values(values);
        widget.state = match total {
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

#[derive(Debug, PartialEq, Eq)]
enum Watched<'a> {
    None,
    Pacman,
    Aur(&'a str),
    Both(&'a str),
}

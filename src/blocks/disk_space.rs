//! Disk usage statistics
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `path` | Path to collect information from. Supports path expansions e.g. `~`. | `"/"`
//! `interval` | Update time in seconds | `20`
//! `format` | A string to customise the output of this block. See below for available placeholders. | `" $icon $available "`
//! `format_alt` | If set, block will switch between `format` and `format_alt` on every click | `None`
//! `warning` | A value which will trigger warning block state | `20.0`
//! `alert` | A value which will trigger critical block state | `10.0`
//! `info_type` | Determines which information will affect the block state. Possible values are `"available"`, `"free"` and `"used"` | `"available"`
//! `alert_unit` | The unit of `alert` and `warning` options. If not set, percents are used. Possible values are `"B"`, `"KB"`, `"KiB"`, `"MB"`, `"MiB"`, `"GB"`, `"Gib"`, `"TB"` and `"TiB"` | `None`
//! `backend` | The backend to use when querying disk usage. Possible values are `"vfs"` (like `du(1)`) and `"btrfs"` | `"vfs"`
//!
//! Placeholder  | Value                                                              | Type   | Unit
//! -------------|--------------------------------------------------------------------|--------|-------
//! `icon`       | A static icon                                                      | Icon   | -
//! `path`       | The value of `path` option                                         | Text   | -
//! `percentage` | Free or used percentage. Depends on `info_type`                    | Number | %
//! `total`      | Total disk space                                                   | Number | Bytes
//! `used`       | Used disk space                                                    | Number | Bytes
//! `free`       | Free disk space                                                    | Number | Bytes
//! `available`  | Available disk space (free disk space minus reserved system space) | Number | Bytes
//!
//! Action          | Description                               | Default button
//! ----------------|-------------------------------------------|---------------
//! `toggle_format` | Toggles between `format` and `format_alt` | Left
//!
//! # Examples
//!
//! ```toml
//! [[block]]
//! block = "disk_space"
//! info_type = "available"
//! alert_unit = "GB"
//! alert = 10.0
//! warning = 15.0
//! format = " $icon $available "
//! format_alt = " $icon $available / $total "
//! ```
//!
//! Update block on right click:
//!
//! ```toml
//! [[block]]
//! block = "disk_space"
//! [[block.click]]
//! button = "right"
//! update = true
//! ```
//!
//! Show the block only if less than 10GB is available:
//!
//! ```toml
//! [[block]]
//! block = "disk_space"
//! format = " $free.eng(range:..10e9) |"
//! ```
//!
//! # Icons Used
//! - `disk_drive`

// make_log_macro!(debug, "disk_space");

use std::cell::OnceCell;

use super::prelude::*;
use crate::formatting::prefix::Prefix;
use nix::sys::statvfs::statvfs;
use tokio::process::Command;

#[derive(Copy, Clone, Debug, Deserialize, SmartDefault)]
#[serde(rename_all = "lowercase")]
pub enum InfoType {
    #[default]
    Available,
    Free,
    Used,
}

#[derive(Copy, Clone, Debug, Deserialize, SmartDefault)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    #[default]
    Vfs,
    Btrfs,
}

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    #[default("/".into())]
    pub path: ShellString,
    pub backend: Backend,
    pub info_type: InfoType,
    pub format: FormatConfig,
    pub format_alt: Option<FormatConfig>,
    pub alert_unit: Option<String>,
    #[default(20.into())]
    pub interval: Seconds,
    #[default(20.0)]
    pub warning: f64,
    #[default(10.0)]
    pub alert: f64,
}

pub async fn run(config: &Config, api: &CommonApi) -> Result<()> {
    let mut actions = api.get_actions()?;
    api.set_default_actions(&[(MouseButton::Left, None, "toggle_format")])?;

    let mut format = config.format.with_default(" $icon $available ")?;
    let mut format_alt = match &config.format_alt {
        Some(f) => Some(f.with_default("")?),
        None => None,
    };

    let unit = match config.alert_unit.as_deref() {
        // Decimal
        Some("TB") => Some(Prefix::Tera),
        Some("GB") => Some(Prefix::Giga),
        Some("MB") => Some(Prefix::Mega),
        Some("KB") => Some(Prefix::Kilo),
        // Binary
        Some("TiB") => Some(Prefix::Tebi),
        Some("GiB") => Some(Prefix::Gibi),
        Some("MiB") => Some(Prefix::Mebi),
        Some("KiB") => Some(Prefix::Kibi),
        // Byte
        Some("B") => Some(Prefix::One),
        // Unknown
        Some(x) => return Err(Error::new(format!("Unknown unit: '{x}'"))),
        None => None,
    };

    let path = config.path.expand()?;

    let mut timer = config.interval.timer();

    loop {
        let mut widget = Widget::new().with_format(format.clone());

        let (total, used, available, free) = match config.backend {
            Backend::Vfs => get_vfs(&*path)?,
            Backend::Btrfs => get_btrfs(&path).await?,
        };

        let result = match config.info_type {
            InfoType::Available => available,
            InfoType::Free => free,
            InfoType::Used => used,
        } as f64;

        let percentage = result / (total as f64) * 100.;
        widget.set_values(map! {
            "icon" => Value::icon("disk_drive"),
            "path" => Value::text(path.to_string()),
            "percentage" => Value::percents(percentage),
            "total" => Value::bytes(total as f64),
            "used" => Value::bytes(used as f64),
            "available" => Value::bytes(available as f64),
            "free" => Value::bytes(free as f64),
        });

        // Send percentage to alert check if we don't want absolute alerts
        let alert_val_in_config_units = match unit {
            Some(p) => p.apply(result),
            None => percentage,
        };

        // Compute state
        widget.state = match config.info_type {
            InfoType::Used => {
                if alert_val_in_config_units >= config.alert {
                    State::Critical
                } else if alert_val_in_config_units >= config.warning {
                    State::Warning
                } else {
                    State::Idle
                }
            }
            InfoType::Free | InfoType::Available => {
                if alert_val_in_config_units <= config.alert {
                    State::Critical
                } else if alert_val_in_config_units <= config.warning {
                    State::Warning
                } else {
                    State::Idle
                }
            }
        };

        api.set_widget(widget)?;

        loop {
            select! {
                _ = timer.tick() => break,
                _ = api.wait_for_update_request() => break,
                Some(action) = actions.recv() => match action.as_ref() {
                    "toggle_format" => {
                        if let Some(format_alt) = &mut format_alt {
                            std::mem::swap(format_alt, &mut format);
                            break;
                        }
                    }
                    _ => (),
                }
            }
        }
    }
}

fn get_vfs<P>(path: &P) -> Result<(u64, u64, u64, u64)>
where
    P: ?Sized + nix::NixPath,
{
    let statvfs = statvfs(path).error("failed to retrieve statvfs")?;

    // Casting to be compatible with 32-bit systems
    #[allow(clippy::unnecessary_cast)]
    {
        let total = (statvfs.blocks() as u64) * (statvfs.fragment_size() as u64);
        let used = ((statvfs.blocks() as u64) - (statvfs.blocks_free() as u64))
            * (statvfs.fragment_size() as u64);
        let available = (statvfs.blocks_available() as u64) * (statvfs.block_size() as u64);
        let free = (statvfs.blocks_free() as u64) * (statvfs.block_size() as u64);

        Ok((total, used, available, free))
    }
}

async fn get_btrfs(path: &str) -> Result<(u64, u64, u64, u64)> {
    const OUTPUT_CHANGED: &str = "Btrfs filesystem usage output format changed";

    fn remove_estimate_min(estimate_str: &str) -> Result<&str> {
        estimate_str
            .trim_matches('\t')
            .split_once("\t")
            .ok_or(Error::new(OUTPUT_CHANGED))
            .map(|v| v.0)
    }

    macro_rules! get {
        ($source:expr, $name:expr, $variable:ident) => {
            get!(@pre_op (|a| {Ok::<_, Error>(a)}), $source, $name, $variable)
        };
        (@pre_op $function:expr, $source:expr, $name:expr, $variable:ident) => {
            if $source.starts_with(concat!($name, ":")) {
                let (found_name, variable_str) =
                    $source.split_once(":").ok_or(Error::new(OUTPUT_CHANGED))?;

                let variable_str = $function(variable_str)?;

                debug_assert_eq!(found_name, $name);
                $variable
                    .set(variable_str.trim().parse().error(OUTPUT_CHANGED)?)
                    .map_err(|_| Error::new(OUTPUT_CHANGED))?;
            }
        };
    }

    let filesystem_usage = Command::new("btrfs")
        .args(["filesystem", "usage", "--raw", path])
        .output()
        .await
        .error("Failed to collect btrfs filesystem usage info")?
        .stdout;

    {
        let final_total = OnceCell::new();
        let final_used = OnceCell::new();
        let final_free = OnceCell::new();

        let mut lines = filesystem_usage.lines();
        while let Some(line) = lines
            .next_line()
            .await
            .error("Failed to read output of btrfs filesystem usage")?
        {
            let line = line.trim();

            // See btrfs-filesystem(8) for an explanation for the rows.
            get!(line, "Device size", final_total);
            get!(line, "Used", final_used);
            get!(@pre_op remove_estimate_min, line, "Free (estimated)", final_free);
        }

        Ok((
            *final_total.get().ok_or(Error::new(OUTPUT_CHANGED))?,
            *final_used.get().ok_or(Error::new(OUTPUT_CHANGED))?,
            // HACK(@bpeetz): We also return the free disk space as the available one, because btrfs
            // does not tell us which disk space is reserved for the fs. <2025-05-18>
            *final_free.get().ok_or(Error::new(OUTPUT_CHANGED))?,
            *final_free.get().ok_or(Error::new(OUTPUT_CHANGED))?,
        ))
    }
}

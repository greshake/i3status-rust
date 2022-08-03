//! Information about the internal power supply
//!
//! This block can display the current battery state (Full, Charging or Discharging), percentage
//! charged and estimate time until (dis)charged for an internal power supply.
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `device` | The device in `/sys/class/power_supply/` to read from. When using UPower, this can also be `"DisplayDevice"`. Regular expressions can be used. | Any battery device
//! `driver` | One of `"sysfs"`, `"apc_ups"`, or `"upower"` | `"sysfs"`
//! `interval` | Update interval, in seconds. Only relevant for `driver = "sysfs"` \|\| "apc_ups"`. | `10`
//! `format` | A string to customise the output of this block. See below for available placeholders. | <code>"$percentage&vert;"</code>
//! `full_format` | Same as `format` but for when the battery is full | `""`
//! `empty_format` | Same as `format` but for when the battery is empty | `""`
//! `hide_missing` | Completely hide this block if the battery cannot be found. | `false`
//! `hide_full` | Hide the block if battery is full | `false`
//! `info` | Minimum battery level, where state is set to info | `60`
//! `good` | Minimum battery level, where state is set to good | `60`
//! `warning` | Minimum battery level, where state is set to warning | `30`
//! `critical` | Minimum battery level, where state is set to critical | `15`
//! `full_threshold` | Percentage above which the battery is considered full (`full_format` shown) | `95`
//! `empty_threshold` | Percentage below which the battery is considered empty | `7.5`
//!
//! Placeholder  | Value                                                                   | Type              | Unit
//! -------------|-------------------------------------------------------------------------|-------------------|-----
//! `percentage` | Battery level, in percent                                               | String or Integer | Percents
//! `time`       | Time remaining until (dis)charge is complete. Presented only if battery's status is (dis)charging. | String | -
//! `power`      | Power consumption by the battery or from the power supply when charging | String or Float   | Watts
//!
//! # Examples
//!
//! Basic usage:
//!
//! ```toml
//! [block]
//! block = "battery"
//! format = "$percentage|N/A"
//! ```
//!
//! Hide missing battery:
//!
//! ```toml
//! [block]
//! block = "battery"
//! hide_missing = true
//! ```
//!
//! # Icons Used
//! - `bat_charging`
//! - `bat_not_available`
//! - `bat_10`
//! - `bat_20`
//! - `bat_30`
//! - `bat_40`
//! - `bat_50`
//! - `bat_60`
//! - `bat_70`
//! - `bat_80`
//! - `bat_90`
//! - `bat_full`

use regex::Regex;
use std::convert::Infallible;
use std::str::FromStr;

use super::prelude::*;
use crate::util::battery_level_icon;

mod apc_ups;
mod sysfs;
mod upower;
mod zbus_upower;

make_log_macro!(debug, "battery");

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
struct BatteryConfig {
    device: Option<String>,
    driver: BatteryDriver,
    #[default(10.into())]
    interval: Seconds,
    format: FormatConfig,
    full_format: FormatConfig,
    empty_format: FormatConfig,
    hide_missing: bool,
    hide_full: bool,
    #[default(60.0)]
    info: f64,
    #[default(60.0)]
    good: f64,
    #[default(30.0)]
    warning: f64,
    #[default(15.0)]
    critical: f64,
    #[default(95.0)]
    full_threshold: f64,
    #[default(7.5)]
    empty_threshold: f64,
}

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(rename_all = "snake_case")]
enum BatteryDriver {
    #[default]
    Sysfs,
    ApcUps,
    Upower,
}

pub async fn run(config: toml::Value, mut api: CommonApi) -> Result<()> {
    let config = BatteryConfig::deserialize(config).config_error()?;
    let format = config.format.with_default("$percentage")?;
    let format_full = config.full_format.with_default("")?;
    let format_empty = config.empty_format.with_default("")?;
    let mut widget = api.new_widget();

    let dev_name = DeviceName::new(config.device)?;
    let mut device: Box<dyn BatteryDevice + Send + Sync> = match config.driver {
        BatteryDriver::Sysfs => Box::new(sysfs::Device::new(dev_name, config.interval)),
        BatteryDriver::ApcUps => Box::new(apc_ups::Device::new(dev_name, config.interval).await?),
        BatteryDriver::Upower => Box::new(upower::Device::new(dev_name).await?),
    };

    loop {
        match device.get_info().await? {
            Some(mut info) => {
                if info.capacity >= config.full_threshold {
                    info.status = BatteryStatus::Full;
                } else if info.capacity <= config.empty_threshold {
                    info.status = BatteryStatus::Empty;
                }

                match info.status {
                    BatteryStatus::Empty => {
                        debug!("Using `empty_format`");
                        widget.set_format(format_empty.clone());
                    }
                    BatteryStatus::Full | BatteryStatus::NotCharging => {
                        debug!("Using `full_format`");
                        widget.set_format(format_full.clone());
                    }
                    _ => {
                        debug!("Using `format`");
                        widget.set_format(format.clone());
                    }
                }

                let mut values = map!("percentage" => Value::percents(info.capacity));
                info.power
                    .map(|p| values.insert("power".into(), Value::watts(p)));
                info.time_remaining.map(|t| {
                    values.insert(
                        "time".into(),
                        Value::text(format!(
                            "{}:{:02}",
                            (t / 3600.) as i32,
                            (t % 3600. / 60.) as i32
                        )),
                    )
                });
                widget.set_values(values);

                let (icon, state) = match (info.status, info.capacity) {
                    (BatteryStatus::Empty, _) => (battery_level_icon(0, false), State::Critical),
                    (BatteryStatus::Full, _) => (battery_level_icon(100, false), State::Idle),
                    (status, capacity) => (
                        battery_level_icon(capacity as u8, status == BatteryStatus::Charging),
                        if status == BatteryStatus::Charging {
                            State::Good
                        } else if capacity <= config.critical {
                            State::Critical
                        } else if capacity <= config.warning {
                            State::Warning
                        } else if capacity <= config.info {
                            State::Info
                        } else if capacity > config.good {
                            State::Good
                        } else {
                            State::Idle
                        },
                    ),
                };

                widget.set_icon(icon)?;
                widget.state = state;
                api.set_widget(&widget).await?;
            }
            None if config.hide_missing => {
                api.hide().await?;
            }
            None => {
                widget.set_icon("bat_not_available")?;
                widget.set_format(format.clone());
                widget.set_values(default());
                widget.state = State::Critical;
                api.set_widget(&widget).await?;
            }
        }

        select! {
            update = device.wait_for_change() => update?,
            _ = api.wait_for_update_request() => (),
        }
    }
}

#[async_trait]
trait BatteryDevice {
    async fn get_info(&mut self) -> Result<Option<BatteryInfo>>;
    async fn wait_for_change(&mut self) -> Result<()>;
}

/// `Option<Regex>`, but more intuitive
#[derive(Debug)]
enum DeviceName {
    Any,
    Regex(Regex),
}

impl DeviceName {
    fn new(pat: Option<String>) -> Result<Self> {
        Ok(match pat {
            None => Self::Any,
            Some(pat) => Self::Regex(pat.parse().error("failed to parse regex")?),
        })
    }

    fn matches(&self, name: &str) -> bool {
        match self {
            Self::Any => true,
            Self::Regex(pat) => pat.is_match(name),
        }
    }

    fn exact(&self) -> Option<&str> {
        match self {
            Self::Any => None,
            Self::Regex(pat) => Some(pat.as_str()),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct BatteryInfo {
    /// Current status, e.g. "charging", "discharging", etc.
    status: BatteryStatus,
    /// The capacity in percents
    capacity: f64,
    /// Power consumption in watts
    power: Option<f64>,
    /// Time in seconds
    time_remaining: Option<f64>,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, SmartDefault)]
enum BatteryStatus {
    Charging,
    Discharging,
    Empty,
    Full,
    NotCharging,
    #[default]
    Unknown,
}

impl FromStr for BatteryStatus {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "Charging" => Self::Charging,
            "Discharging" => Self::Discharging,
            "Empty" => Self::Empty,
            "Full" => Self::Full,
            "Not charging" => Self::NotCharging,
            _ => Self::Unknown,
        })
    }
}

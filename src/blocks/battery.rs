//! Information about the internal power supply
//!
//! This block can display the current battery state (Full, Charging or Discharging), percentage
//! charged and estimate time until (dis)charged for an internal power supply.
//!
//! # Configuration
//!
//! Key | Values | Required | Default
//! ----|--------|----------|--------
//! `device` | The device in `/sys/class/power_supply/` to read from. When using UPower, this can also be `"DisplayDevice"`. Regular expressions can be used. | No | Any battery device
//! `driver` | One of `"sysfs"` or `"upower"` | No | `"sysfs"`
//! `interval` | Update interval, in seconds. Only relevant for `driver = "sysfs"`. | No | `10`
//! `format` | A string to customise the output of this block. See below for available placeholders. | No | <code>"$percentage&vert;"</code>
//! `full_format` | Same as `format` but for when the battery is full | No | `""`
//! `hide_missing` | Completely hide this block if the battery cannot be found. | No | `false`
//! `hide_full` | Hide the block if battery is full | No | `false`
//! `info` | Minimum battery level, where state is set to info | No | `60`
//! `good` | Minimum battery level, where state is set to good | No | `60`
//! `warning` | Minimum battery level, where state is set to warning | No | `30`
//! `critical` | Minimum battery level, where state is set to critical | No | `15`
//! `full_threshold` | Percentage at which the battery is considered full (`full_format` shown) | No | `100`
//!
//! Placeholder  | Value                                                                   | Type              | Unit
//! -------------|-------------------------------------------------------------------------|-------------------|-----
//! `percentage` | Battery level, in percent                                               | String or Integer | Percents
//! `time`       | Time remaining until (dis)charge is complete. Presented only if battery's status is (dis)charging. | String | -
//! `power`      | Power consumption by the battery or from the power supply when charging | String or Float   | Watts
//!
//! # Examples
//!
//! Hide missing battery:
//!
//! ```toml
//! [block]
//! block = "battery"
//! hide_missing = true
//! ```
//!
//! Allow missing battery:
//!
//! ```toml
//! [block]
//! block = "battery"
//! format = "$percentage|N/A"
//! ```
//!
//! # Icons Used
//! - `bat_charging`
//! - `bat_not_available`
//! - `bat_10`,
//! - `bat_20`,
//! - `bat_30`,
//! - `bat_40`,
//! - `bat_50`,
//! - `bat_60`,
//! - `bat_70`,
//! - `bat_80`,
//! - `bat_90`,
//! - `bat_full`,

use regex::Regex;
use std::convert::Infallible;
use std::str::FromStr;

use super::prelude::*;
use crate::util::battery_level_icon;

mod sysfs;
mod upower;
mod zbus_upower;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
struct BatteryConfig {
    device: Option<String>,
    driver: BatteryDriver,
    #[default(10.into())]
    interval: Seconds,
    format: FormatConfig,
    full_format: FormatConfig,
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
    #[default(100.0)]
    full_threshold: f64,
}

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(rename_all = "lowercase")]
enum BatteryDriver {
    #[default]
    Sysfs,
    Upower,
}

pub async fn run(config: toml::Value, mut api: CommonApi) -> Result<()> {
    let config = BatteryConfig::deserialize(config).config_error()?;
    let format = config.format.with_default("$percentage")?;
    let format_full = config.full_format.with_default("")?;
    let mut widget = api.new_widget();

    let dev_name = DeviceName::new(config.device)?;
    let mut device: Box<dyn BatteryDevice + Send + Sync> = match config.driver {
        BatteryDriver::Sysfs => Box::new(sysfs::Device::new(dev_name, config.interval)),
        BatteryDriver::Upower => Box::new(upower::Device::new(dev_name).await?),
    };

    loop {
        match device.get_info().await? {
            Some(mut info) => {
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

                if info.capacity >= config.full_threshold {
                    info.status = BatteryStatus::Full;
                }

                widget.set_format(match info.status {
                    BatteryStatus::Full | BatteryStatus::NotCharging => format_full.clone(),
                    _ => format.clone(),
                });

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
            }
            None if config.hide_missing => {
                api.hide().await?;
            }
            None => {
                widget.set_icon("bat_not_available")?;
                widget.set_values(default());
                widget.set_format(format.clone());
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

//! Information about the internal power supply
//!
//! This block can display the current battery state (Full, Charging or Discharging), percentage
//! charged and estimate time until (dis)charged for an internal power supply.
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `device` | sysfs/UPower: The device in `/sys/class/power_supply/` to read from (can also be "DisplayDevice" for UPower, which is a single logical power source representing all physical power sources. This is for example useful if your system has multiple batteries, in which case the DisplayDevice behaves as if you had a single larger battery.). apc_ups: IPv4Address:port or hostname:port | sysfs: the first battery device found in /sys/class/power_supply, with "BATx" or "CMBx" entries taking precedence. apc_ups: "localhost:3551". upower: `DisplayDevice`
//! `driver` | One of `"sysfs"`, `"apc_ups"`, or `"upower"` | `"sysfs"`
//! `model` | If present, the contents of `/sys/class/power_supply/.../model_name` must match this value. Typical use is to select by model name on devices that change their path. | N/A
//! `interval` | Update interval, in seconds. Only relevant for `driver = "sysfs"` \|\| "apc_ups"`. | `10`
//! `format` | A string to customise the output of this block. See below for available placeholders. | `" $icon $percentage "`
//! `full_format` | Same as `format` but for when the battery is full | `" $icon "`
//! `empty_format` | Same as `format` but for when the battery is empty | `" $icon "`
//! `not_charging_format` | Same as `format` but for when the battery is not charging. Defaults to the full battery icon as many batteries report this status when they are full. | `" $icon "`
//! `missing_format` | Same as `format` if the battery cannot be found. | `" $icon "`
//! `info` | Minimum battery level, where state is set to info | `60`
//! `good` | Minimum battery level, where state is set to good | `60`
//! `warning` | Minimum battery level, where state is set to warning | `30`
//! `critical` | Minimum battery level, where state is set to critical | `15`
//! `full_threshold` | Percentage above which the battery is considered full (`full_format` shown) | `95`
//! `empty_threshold` | Percentage below which the battery is considered empty | `7.5`
//!
//! Placeholder  | Value                                                                   | Type              | Unit
//! -------------|-------------------------------------------------------------------------|-------------------|-----
//! `icon`       | Icon based on battery's state                                           | Icon   | -
//! `percentage` | Battery level, in percent                                               | Number | Percents
//! `time`       | Time remaining until (dis)charge is complete. Presented only if battery's status is (dis)charging. | String | -
//! `power`      | Power consumption by the battery or from the power supply when charging | String or Float   | Watts
//!
//! # Examples
//!
//! Basic usage:
//!
//! ```toml
//! [[block]]
//! block = "battery"
//! format = " $icon $percentage "
//! ```
//!
//! ```toml
//! [[block]]
//! block = "battery"
//! format = " $percentage {$time |}"
//! device = "DisplayDevice"
//! driver = "upower"
//! ```
//!
//! Hide missing battery:
//!
//! ```toml
//! [[block]]
//! block = "battery"
//! missing_format = ""
//! ```
//!
//! # Icons Used
//! - `bat` (as a progression)
//! - `bat_charging` (as a progression)
//! - `bat_not_available`

use regex::Regex;
use std::convert::Infallible;
use std::str::FromStr;

use super::prelude::*;

mod apc_ups;
mod sysfs;
mod upower;

// make_log_macro!(debug, "battery");

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(default)]
pub struct Config {
    device: Option<String>,
    driver: BatteryDriver,
    model: Option<String>,
    #[default(10.into())]
    interval: Seconds,
    format: FormatConfig,
    full_format: FormatConfig,
    empty_format: FormatConfig,
    not_charging_format: FormatConfig,
    missing_format: FormatConfig,
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

pub async fn run(config: Config, mut api: CommonApi) -> Result<()> {
    let format = config.format.with_default(" $icon $percentage ")?;
    let format_full = config.full_format.with_default(" $icon ")?;
    let format_empty = config.empty_format.with_default(" $icon ")?;
    let format_not_charging = config.not_charging_format.with_default(" $icon ")?;
    let missing_format = config.missing_format.with_default(" $icon ")?;
    let mut widget = Widget::new();

    let dev_name = DeviceName::new(config.device)?;
    let mut device: Box<dyn BatteryDevice + Send + Sync> = match config.driver {
        BatteryDriver::Sysfs => {
            Box::new(sysfs::Device::new(dev_name, config.model, config.interval))
        }
        BatteryDriver::ApcUps => Box::new(apc_ups::Device::new(dev_name, config.interval).await?),
        BatteryDriver::Upower => Box::new(upower::Device::new(dev_name, config.model).await?),
    };

    loop {
        let mut info = device.get_info().await?;

        if let Some(info) = &mut info {
            if info.capacity >= config.full_threshold {
                info.status = BatteryStatus::Full;
            } else if info.capacity <= config.empty_threshold {
                info.status = BatteryStatus::Empty;
            }
        }

        match info {
            Some(info) => {
                widget.set_format(match info.status {
                    BatteryStatus::Empty => format_empty.clone(),
                    BatteryStatus::Full => format_full.clone(),
                    BatteryStatus::NotCharging => format_not_charging.clone(),
                    _ => format.clone(),
                });

                let mut values = map!(
                    "percentage" => Value::percents(info.capacity)
                );

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

                let (icon_name, icon_value, state) = match (info.status, info.capacity) {
                    (BatteryStatus::Empty, _) => ("bat", 0.0, State::Critical),
                    (BatteryStatus::Full | BatteryStatus::NotCharging, _) => {
                        ("bat", 1.0, State::Idle)
                    }
                    (status, capacity) => (
                        if status == BatteryStatus::Charging {
                            "bat_charging"
                        } else {
                            "bat"
                        },
                        capacity / 100.0,
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

                values.insert(
                    "icon".into(),
                    Value::icon(api.get_icon_in_progression(icon_name, icon_value)?),
                );

                widget.set_values(values);
                widget.state = state;
                api.set_widget(&widget).await?;
            }
            None => {
                widget.set_format(missing_format.clone());
                widget.set_values(map!("icon" => Value::icon(api.get_icon("bat_not_available")?)));
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

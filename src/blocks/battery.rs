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
//! `interval` | Update interval, in seconds. Only relevant for driver = "sysfs" or "apc_ups". | `10`
//! `format` | A string to customise the output of this block. See below for available placeholders. | `" $icon $percentage "`
//! `full_format` | Same as `format` but for when the battery is full | `" $icon "`
//! `charging_format` | Same as `format` but for when the battery is charging | Links to `format`
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
//! `time_remaining`  | Time remaining until (dis)charge is complete. Presented only if battery's status is (dis)charging. | Duration | -
//! `time`       | Time remaining until (dis)charge is complete. Presented only if battery's status is (dis)charging. | String *DEPRECATED* | -
//! `power`      | Power consumption by the battery or from the power supply when charging | String or Float   | Watts
//!
//! `time` has been deprecated in favor of `time_remaining`.
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
//! format = " $percentage {$time_remaining.dur(hms:true, min_unit:m) |}"
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
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub device: Option<String>,
    pub driver: BatteryDriver,
    pub model: Option<String>,
    #[default(10.into())]
    pub interval: Seconds,
    pub format: FormatConfig,
    pub full_format: FormatConfig,
    pub charging_format: FormatConfig,
    pub empty_format: FormatConfig,
    pub not_charging_format: FormatConfig,
    pub missing_format: FormatConfig,
    #[default(60.0)]
    pub info: f64,
    #[default(60.0)]
    pub good: f64,
    #[default(30.0)]
    pub warning: f64,
    #[default(15.0)]
    pub critical: f64,
    #[default(95.0)]
    pub full_threshold: f64,
    #[default(7.5)]
    pub empty_threshold: f64,
}

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(rename_all = "snake_case")]
pub enum BatteryDriver {
    #[default]
    Sysfs,
    ApcUps,
    Upower,
}

pub async fn run(config: &Config, api: &CommonApi) -> Result<()> {
    let format = config.format.with_default(" $icon $percentage ")?;
    let format_full = config.full_format.with_default(" $icon ")?;
    let charging_format = config.charging_format.with_default_format(&format);
    let format_empty = config.empty_format.with_default(" $icon ")?;
    let format_not_charging = config.not_charging_format.with_default(" $icon ")?;
    let missing_format = config.missing_format.with_default(" $icon ")?;

    let dev_name = DeviceName::new(config.device.clone())?;
    let mut device: Box<dyn BatteryDevice + Send + Sync> = match config.driver {
        BatteryDriver::Sysfs => Box::new(sysfs::Device::new(
            dev_name,
            config.model.clone(),
            config.interval,
        )),
        BatteryDriver::ApcUps => Box::new(apc_ups::Device::new(dev_name, config.interval).await?),
        BatteryDriver::Upower => {
            Box::new(upower::Device::new(dev_name, config.model.clone()).await?)
        }
    };

    loop {
        let mut info = device.get_info().await?;

        if let Some(info) = &mut info {
            if info.capacity >= config.full_threshold {
                info.status = BatteryStatus::Full;
            } else if info.capacity <= config.empty_threshold
                && info.status != BatteryStatus::Charging
            {
                info.status = BatteryStatus::Empty;
            }
        }

        match info {
            Some(info) => {
                let mut widget = Widget::new();

                widget.set_format(match info.status {
                    BatteryStatus::Empty => format_empty.clone(),
                    BatteryStatus::Full => format_full.clone(),
                    BatteryStatus::Charging => charging_format.clone(),
                    BatteryStatus::NotCharging => format_not_charging.clone(),
                    _ => format.clone(),
                });

                let mut values = map!(
                    "percentage" => Value::percents(info.capacity)
                );

                info.power
                    .map(|p| values.insert("power".into(), Value::watts(p)));
                info.time_remaining.inspect(|&t| {
                    map! { @extend values
                        "time" => Value::text(
                            format!(
                                "{}:{:02}",
                                (t / 3600.) as i32,
                                (t % 3600. / 60.) as i32
                            ),
                        ),
                        "time_remaining" =>  Value::duration(
                            Duration::from_secs(t as u64),
                        ),
                    }
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
                    Value::icon_progression(icon_name, icon_value),
                );

                widget.set_values(values);
                widget.state = state;
                api.set_widget(widget)?;
            }
            None => {
                let mut widget = Widget::new()
                    .with_format(missing_format.clone())
                    .with_state(State::Critical);
                widget.set_values(map!("icon" => Value::icon("bat_not_available")));
                api.set_widget(widget)?;
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

//! Information about the internal power supply
//!
//! This block can display the current battery state (Full, Charging or Discharging), percentage
//! charged and estimate time until (dis)charged for an internal power supply.
//!
//! # Configuration
//!
//! Key | Values | Required | Default
//! ----|--------|----------|--------
//! `device` | The device in `/sys/class/power_supply/` to read from. When using UPower, this can also be `"DisplayDevice"`. | No | Any battery device
//! `driver` | One of `"sysfs"` or `"upower"` | No | `"sysfs"`
//! `interval` | Update interval, in seconds. Only relevant for `driver = "sysfs"`. | No | `10`
//! `format` | A string to customise the output of this block. See below for available placeholders. | No | <code>"$percentage&vert;"</code>
//! `full_format` | Same as `format` but for when the battery is full | No | `""`
//! `allow_missing` | Don't display errors when the battery cannot be found. Only works with the `sysfs` driver. | No | `false`
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
//! allow_missing = true
//! ```
//!
//! # Icons Used
//! - `bat_charging`
//! - `bat_not_available`
//! - "bat_10",
//! - "bat_20",
//! - "bat_30",
//! - "bat_40",
//! - "bat_50",
//! - "bat_60",
//! - "bat_70",
//! - "bat_80",
//! - "bat_90",
//! - "bat_full",

use std::collections::HashMap;
use std::convert::Infallible;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use async_trait::async_trait;
use tokio::fs::{read_dir, read_to_string};
use tokio::time::Interval;
use zbus::fdo::DBusProxy;
use zbus::MessageStream;

use super::prelude::*;
use crate::util::{battery_level_icon, new_system_dbus_connection, read_file};

mod zbus_upower;

/// Path for the power supply devices
const POWER_SUPPLY_DEVICES_PATH: &str = "/sys/class/power_supply";

/// Ordered list of icons used to display battery charge
const BATTERY_UNAVAILABLE_ICON: &str = "bat_not_available";

#[derive(Deserialize, Debug, Derivative)]
#[serde(deny_unknown_fields, default)]
#[derivative(Default)]
struct BatteryConfig {
    device: Option<StdString>,
    driver: BatteryDriver,
    #[derivative(Default(value = "10.into()"))]
    interval: Seconds,
    format: FormatConfig,
    full_format: FormatConfig,
    allow_missing: bool,
    hide_missing: bool,
    hide_full: bool,
    #[derivative(Default(value = "60.0"))]
    info: f64,
    #[derivative(Default(value = "60.0"))]
    good: f64,
    #[derivative(Default(value = "30.0"))]
    warning: f64,
    #[derivative(Default(value = "15.0"))]
    critical: f64,
    #[derivative(Default(value = "100.0"))]
    full_threshold: f64,
}

#[derive(Deserialize, Debug, Derivative)]
#[serde(rename_all = "lowercase")]
#[derivative(Default)]
enum BatteryDriver {
    #[derivative(Default)]
    Sysfs,
    Upower,
}

pub async fn run(config: toml::Value, mut api: CommonApi) -> Result<()> {
    let config = BatteryConfig::deserialize(config).config_error()?;
    let format = config.format.with_default("$percentage")?;
    let format_full = config.full_format.with_default("")?;

    // Get _any_ battery device if not set in the config
    let device = match config.device {
        Some(d) => d,
        None => {
            let mut sysfs_dir = read_dir("/sys/class/power_supply")
                .await
                .error("failed to read /sys/class/power_supply direcory")?;
            let mut device = None;
            while let Some(dir) = sysfs_dir
                .next_entry()
                .await
                .error("failed to read /sys/class/power_supply direcory")?
            {
                if read_to_string(dir.path().join("type"))
                    .await
                    .map(|t| t.trim() == "Battery")
                    .unwrap_or(false)
                {
                    device = Some(dir.file_name().to_str().unwrap().to_string());
                    break;
                }
            }
            device.error("failed to determine default battery - please set your battery device in the configuration file")?
        }
    };

    let dbus_conn;
    let mut device: Box<dyn BatteryDevice + Send + Sync> = match config.driver {
        BatteryDriver::Sysfs => Box::new(PowerSupplyDevice::from_device(&device, config.interval)),
        BatteryDriver::Upower => {
            dbus_conn = new_system_dbus_connection().await?;
            Box::new(UPowerDevice::from_device(&device, &dbus_conn).await?)
        }
    };

    loop {
        match device.get_info().await? {
            Some(mut info) => {
                api.show();

                let mut values = map!("percentage" => Value::percents(info.capacity));
                info.power
                    .map(|p| values.insert("power".into(), Value::watts(p)));
                info.time_remaining.map(|t| {
                    values.insert(
                        "time".into(),
                        Value::text(
                            format!("{}:{:02}", (t / 3600.) as i32, (t % 3600. / 60.) as i32)
                                .into(),
                        ),
                    )
                });
                api.set_values(values);

                if info.capacity >= config.full_threshold {
                    info.status = BatteryStatus::Full;
                }

                if matches!(
                    info.status,
                    BatteryStatus::Full | BatteryStatus::NotCharging
                ) {
                    api.set_format(format_full.clone());
                } else {
                    api.set_format(format.clone());
                }

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

                api.set_icon(icon)?;
                api.set_state(state);
            }
            None if config.hide_missing => {
                api.hide();
            }
            None if config.allow_missing => {
                api.show();
                api.set_icon(BATTERY_UNAVAILABLE_ICON)?;
                api.set_values(HashMap::new());
                api.set_format(format.clone());
            }
            None => return Err(Error::new("Missing battery")),
        }

        api.flush().await?;
        device.wait_for_change().await?
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

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum BatteryStatus {
    Charging,
    Discharging,
    Empty,
    Full,
    NotCharging,
    Unknown,
}

impl Default for BatteryStatus {
    fn default() -> Self {
        Self::Unknown
    }
}

impl FromStr for BatteryStatus {
    type Err = Infallible;

    fn from_str(s: &str) -> StdResult<Self, Infallible> {
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

#[async_trait]
trait BatteryDevice {
    async fn get_info(&self) -> Result<Option<BatteryInfo>>;
    async fn wait_for_change(&mut self) -> Result<()>;
}

/// Represents a physical power supply device, as known to sysfs.
/// <https://www.kernel.org/doc/html/v5.15/power/power_supply_class.html>
struct PowerSupplyDevice {
    device_path: PathBuf,
    interval: Interval,
}

impl PowerSupplyDevice {
    fn from_device(device: &str, interval: Seconds) -> Self {
        Self {
            device_path: Path::new(POWER_SUPPLY_DEVICES_PATH).join(device),
            interval: interval.timer(),
        }
    }

    async fn read_prop<T>(&self, prop: &str) -> Option<T>
    where
        T: FromStr + Send + Sync,
    {
        read_file(&self.device_path.join(prop))
            .await
            .ok()
            .and_then(|x| x.parse().ok())
    }

    async fn present(&self) -> bool {
        self.read_prop::<u8>("present").await == Some(1)
    }
}

#[async_trait]
impl BatteryDevice for PowerSupplyDevice {
    async fn get_info(&self) -> Result<Option<BatteryInfo>> {
        // Check if the battery is available
        if !self.present().await {
            return Ok(None);
        }

        // Read all the necessary data
        let (
            status,
            capacity,
            charge_now,
            charge_full,
            energy_now,
            energy_full,
            power_now,
            current_now,
            voltage_now,
            time_to_empty,
            time_to_full,
        ) = tokio::join!(
            self.read_prop::<BatteryStatus>("status"),
            self.read_prop::<f64>("capacity"),
            self.read_prop::<f64>("charge_now"),    // uAh
            self.read_prop::<f64>("charge_full"),   // uAh
            self.read_prop::<f64>("energy_now"),    // uWh
            self.read_prop::<f64>("energy_full"),   // uWh
            self.read_prop::<f64>("power_now"),     // uW
            self.read_prop::<f64>("current_now"),   // uA
            self.read_prop::<f64>("voltage_now"),   // uV
            self.read_prop::<f64>("time_to_empty"), // seconds
            self.read_prop::<f64>("time_to_full"),  // seconds
        );

        let charge_now = charge_now.map(|c| c * 1e-6); // uAh -> Ah
        let charge_full = charge_full.map(|c| c * 1e-6); // uAh -> Ah
        let energy_now = energy_now.map(|e| e * 1e-6); // uWh -> Wh
        let energy_full = energy_full.map(|e| e * 1e-6); // uWh -> Wh
        let power_now = power_now.map(|e| e * 1e-6); // uW -> W
        let current_now = current_now.map(|e| e * 1e-6); // uA -> A
        let voltage_now = voltage_now.map(|e| e * 1e-6); // uV -> V

        let status = status.unwrap_or_default();

        let calc_capacity = |(now, full)| (now / full * 100.0);
        let capacity = capacity
            .or_else(|| charge_now.zip(charge_full).map(calc_capacity))
            .or_else(|| energy_now.zip(energy_full).map(calc_capacity))
            .error("Failed to get capacity")?;

        // A * V = W
        let power = power_now.or_else(|| current_now.zip(voltage_now).map(|(c, v)| c * v));

        // Ah * V = Wh
        // Wh / W = h
        let time_remaining = match status {
            BatteryStatus::Charging =>
            {
                #[allow(clippy::unnecessary_lazy_evaluations)]
                time_to_full.or_else(|| match (energy_now, energy_full, power) {
                    (Some(en), Some(ef), Some(p)) => Some((ef - en) / p * 3600.0),
                    _ => match (charge_now, charge_full, voltage_now, power) {
                        (Some(cn), Some(cf), Some(v), Some(p)) => Some((cf - cn) * v / p * 3600.0),
                        _ => None,
                    },
                })
            }
            BatteryStatus::Discharging =>
            {
                #[allow(clippy::unnecessary_lazy_evaluations)]
                time_to_empty.or_else(|| match (energy_now, power) {
                    (Some(en), Some(p)) => Some(en / p * 3600.0),
                    _ => match (charge_now, voltage_now, power) {
                        (Some(cn), Some(v), Some(p)) => Some(cn * v / p * 3600.0),
                        _ => None,
                    },
                })
            }
            _ => None,
        };

        Ok(Some(BatteryInfo {
            status,
            capacity,
            power,
            time_remaining,
        }))
    }

    async fn wait_for_change(&mut self) -> Result<()> {
        self.interval.tick().await;
        Ok(())
    }
}

pub struct UPowerDevice<'a> {
    device_proxy: zbus_upower::DeviceProxy<'a>,
    changes: MessageStream,
}

impl<'a> UPowerDevice<'a> {
    async fn from_device(
        device: &str,
        dbus_conn: &'a zbus::Connection,
    ) -> Result<UPowerDevice<'a>> {
        // Fetch device path
        let device_path = {
            if device == "DisplayDevice" {
                "/org/freedesktop/UPower/devices/DisplayDevice"
                    .try_into()
                    .unwrap()
            } else {
                zbus_upower::UPowerProxy::new(dbus_conn)
                    .await
                    .error("Failed to create UPwerProxy")?
                    .enumerate_devices()
                    .await
                    .error("Failed to retrieve UPower devices")?
                    .into_iter()
                    .find(|entry| entry.ends_with(device))
                    .error("UPower device could not be found")?
            }
        };

        let device_proxy = zbus_upower::DeviceProxy::builder(dbus_conn)
            .path(device_path.clone())
            .error("Failed to set proxy's path")?
            .build()
            .await
            .error("Failed to create DeviceProxy")?;

        // Verify device name
        // https://upower.freedesktop.org/docs/Device.html#Device:Type
        // consider any peripheral, UPS and internal battery
        let device_type = device_proxy
            .type_()
            .await
            .error("Failed to get device's type")?;
        if device_type == 1 {
            return Err(Error::new("UPower device is not a battery."));
        }

        DBusProxy::new(dbus_conn)
            .await
            .error("failed to cerate DBusProxy")?
            .add_match(&format!("type='signal',interface='org.freedesktop.DBus.Properties',member='PropertiesChanged',path='{}'", device_path.as_str()))
            .await
            .error("Failed to add match")?;
        let changes = MessageStream::from(dbus_conn);

        Ok(Self {
            device_proxy,
            changes,
        })
    }
}

#[async_trait]
impl<'a> BatteryDevice for UPowerDevice<'a> {
    async fn get_info(&self) -> Result<Option<BatteryInfo>> {
        let capacity = self
            .device_proxy
            .percentage()
            .await
            .error("Failed to get capacity")?;

        let power = self
            .device_proxy
            .energy_rate()
            .await
            .error("Failed to get power")?;

        let status = match self
            .device_proxy
            .state()
            .await
            .error("Failed to get status")?
        {
            1 => BatteryStatus::Charging,
            2 | 6 => BatteryStatus::Discharging,
            3 => BatteryStatus::Empty,
            4 => BatteryStatus::Full,
            5 => BatteryStatus::NotCharging,
            _ => BatteryStatus::Unknown,
        };

        let time_remaining = match status {
            BatteryStatus::Charging => Some(
                self.device_proxy
                    .time_to_full()
                    .await
                    .error("Failed to get time to full")? as f64,
            ),
            BatteryStatus::Discharging => Some(
                self.device_proxy
                    .time_to_empty()
                    .await
                    .error("Failed to get time to empty")? as f64,
            ),
            _ => None,
        };

        Ok(Some(BatteryInfo {
            status,
            capacity,
            power: Some(power),
            time_remaining,
        }))
    }

    async fn wait_for_change(&mut self) -> Result<()> {
        self.changes.next().await;
        Ok(())
    }
}

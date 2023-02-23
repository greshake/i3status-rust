use std::convert::Infallible;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use tokio::fs::read_dir;
use tokio::time::Interval;

use super::{BatteryDevice, BatteryInfo, BatteryStatus, DeviceName};
use crate::blocks::prelude::*;
use crate::util::read_file;

make_log_macro!(debug, "battery");

/// Path for the power supply devices
const POWER_SUPPLY_DEVICES_PATH: &str = "/sys/class/power_supply";

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum CapacityLevel {
    Full,
    High,
    Normal,
    Low,
    Critical,
    Unknown,
}

impl FromStr for CapacityLevel {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "Full" => Self::Full,
            "High" => Self::High,
            "Normal" => Self::Normal,
            "Low" => Self::Low,
            "Critical" => Self::Critical,
            _ => Self::Unknown,
        })
    }
}

impl CapacityLevel {
    fn percentage(self) -> Option<f64> {
        match self {
            CapacityLevel::Full => Some(100.0),
            CapacityLevel::High => Some(75.0),
            CapacityLevel::Normal => Some(50.0),
            CapacityLevel::Low => Some(25.0),
            CapacityLevel::Critical => Some(5.0),
            CapacityLevel::Unknown => None,
        }
    }
}

/// Represents a physical power supply device, as known to sysfs.
/// <https://www.kernel.org/doc/html/v5.15/power/power_supply_class.html>
pub(super) struct Device {
    dev_name: DeviceName,
    dev_path: Option<PathBuf>,
    interval: Interval,
}

impl Device {
    pub(super) fn new(dev_name: DeviceName, interval: Seconds) -> Self {
        Self {
            dev_name,
            dev_path: None,
            interval: interval.timer(),
        }
    }

    /// Returns `self.dev_path` if it is still available. Otherwise, find any device that matches
    /// `self.dev_name`.
    async fn get_device_path(&mut self) -> Result<Option<&Path>> {
        if let Some(path) = &self.dev_path {
            if Self::device_available(path).await {
                debug!("battery '{}' is still available", path.display());
                return Ok(self.dev_path.as_deref());
            }
        }

        let mut matching_battery = None;

        let mut sysfs_dir = read_dir(POWER_SUPPLY_DEVICES_PATH)
            .await
            .error("failed to read /sys/class/power_supply direcory")?;
        while let Some(dir) = sysfs_dir
            .next_entry()
            .await
            .error("failed to read /sys/class/power_supply direcory")?
        {
            let name = dir.file_name();
            let name = name.to_str().error("non UTF-8 battery path")?;

            let path = dir.path();

            if !self.dev_name.matches(name)
                || Self::read_prop::<String>(&path, "type").await.as_deref() != Some("Battery")
                || !Self::device_available(&path).await
            {
                continue;
            }

            debug!(
                "Found matching battery: '{}' matches {:?}",
                path.display(),
                self.dev_name
            );

            // Better to default to the system battery, rather than possibly a keyboard or mouse battery.
            // System batteries usually start with BAT or CMB.
            if name.starts_with("BAT") || name.starts_with("CMB") {
                return Ok(Some(self.dev_path.insert(path)));
            } else {
                matching_battery = Some(path);
            }
        }

        Ok(match matching_battery {
            Some(path) => Some(self.dev_path.insert(path)),
            None => {
                debug!("No batteries found");
                None
            }
        })
    }

    async fn read_prop<T: FromStr + Send + Sync>(path: &Path, prop: &str) -> Option<T> {
        read_file(path.join(prop))
            .await
            .ok()
            .and_then(|x| x.parse().ok())
    }

    async fn device_available(path: &Path) -> bool {
        // If `scope` is `Device`, then this is HID, in which case we don't have to check the
        // `present` property, because the existence of the device direcory implies that the device
        // is available
        Self::read_prop::<String>(path, "scope").await.as_deref() == Some("Device")
            || Self::read_prop::<u8>(path, "present").await == Some(1)
    }
}

#[async_trait]
impl BatteryDevice for Device {
    async fn get_info(&mut self) -> Result<Option<BatteryInfo>> {
        // Check if the battery is available
        let path = match self.get_device_path().await? {
            Some(path) => path,
            None => return Ok(None),
        };

        // Read all the necessary data
        let (
            status,
            capacity_level,
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
            Self::read_prop::<BatteryStatus>(path, "status"),
            Self::read_prop::<CapacityLevel>(path, "capacity_level"),
            Self::read_prop::<f64>(path, "capacity"),
            Self::read_prop::<f64>(path, "charge_now"), // uAh
            Self::read_prop::<f64>(path, "charge_full"), // uAh
            Self::read_prop::<f64>(path, "energy_now"), // uWh
            Self::read_prop::<f64>(path, "energy_full"), // uWh
            Self::read_prop::<f64>(path, "power_now"),  // uW
            Self::read_prop::<f64>(path, "current_now"), // uA
            Self::read_prop::<f64>(path, "voltage_now"), // uV
            Self::read_prop::<f64>(path, "time_to_empty"), // seconds
            Self::read_prop::<f64>(path, "time_to_full"), // seconds
        );

        if !Self::device_available(path).await {
            // Device became unavailable while we were reading data from it. The simplest thing we
            // can do now is to pretend it wasn't available to begin with.
            debug!("battery suddenly unavailable");
            return Ok(None);
        }

        debug!("status = {:?}", status);
        debug!("capacity_level = {:?}", capacity_level);
        debug!("capacity= {:?}", capacity);
        debug!("charge_now = {:?}", charge_now);
        debug!("charge_full = {:?}", charge_full);
        debug!("energy_now = {:?}", energy_now);
        debug!("energy_full = {:?}", energy_full);
        debug!("power_now = {:?}", power_now);
        debug!("current_now = {:?}", current_now);
        debug!("voltage_now = {:?}", voltage_now);
        debug!("time_to_empty = {:?}", time_to_empty);
        debug!("time_to_full = {:?}", time_to_full);

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
            .or_else(|| capacity_level.and_then(CapacityLevel::percentage))
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

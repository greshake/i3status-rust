//! A block for displaying information about an internal power supply.
//!
//! This module contains the [`Battery`](./struct.Battery.html) block, which can
//! display the status, capacity, and time remaining for (dis)charge for an
//! internal power supply.

use std::collections::HashMap;
use std::fs::{read_dir, read_to_string};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use dbus::arg::Array;
use dbus::ffidisp::stdintf::org_freedesktop_dbus::Properties;
use serde_derive::Deserialize;

use crate::apcaccess::ApcAccess;
use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::SharedConfig;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::formatting::value::Value;
use crate::formatting::FormatTemplate;
use crate::scheduler::Task;
use crate::util::{battery_level_to_icon, read_file};
use crate::widgets::text::TextWidget;
use crate::widgets::{I3BarWidget, State};

/// A battery device can be queried for a few properties relevant to the user.
pub trait BatteryDevice {
    /// Query whether the device is available. Batteries can be hot-swappable
    /// and configurations may be used for multiple devices (desktop AND laptop).
    fn is_available(&self) -> bool;

    /// Logic for getting some static battery specs.
    /// An error may be thrown when these specs cannot be found or
    /// if the device is unexpectedly missing.
    ///
    /// Batteries can be hot-swappable, meaning that they can go away and be replaced
    /// with another battery with differrent specs.
    fn refresh_device_info(&mut self) -> Result<()>;

    /// Query the device status. One of `"Full"`, `"Charging"`, `"Discharging"`,
    /// or `"Unknown"`. Thinkpad batteries also report "`Not charging`", which
    /// for our purposes should be treated as equivalent to full.
    fn status(&self) -> Result<String>;

    /// Query the device's current capacity, as a percent.
    fn capacity(&self) -> Result<u64>;

    /// Query the estimated time remaining, in minutes, before (dis)charging is
    /// complete.
    fn time_remaining(&self) -> Result<u64>;

    /// Query the current power consumption, in μW.
    fn power_consumption(&self) -> Result<u64>;
}

/// Represents a physical power supply device, as known to sysfs.
pub struct PowerSupplyDevice {
    device_path: PathBuf,
    allow_missing: bool,
    charge_full: Option<u64>,
    energy_full: Option<u64>,
}

impl PowerSupplyDevice {
    /// Use the power supply device `device`, as found in the
    /// `/sys/class/power_supply` directory. Raises an error if the directory for
    /// that device cannot be found and `allow_missing` is `false`.
    pub fn from_device(device: &str, allow_missing: bool) -> Result<Self> {
        let device_path = Path::new("/sys/class/power_supply").join(device);

        let device = PowerSupplyDevice {
            device_path,
            allow_missing,
            charge_full: None,
            energy_full: None,
        };

        Ok(device)
    }
}

impl BatteryDevice for PowerSupplyDevice {
    fn is_available(&self) -> bool {
        self.device_path.exists()
    }

    fn refresh_device_info(&mut self) -> Result<()> {
        if !self.is_available() {
            // The user indicated that it's ok for this battery to be missing/go away
            if self.allow_missing {
                self.charge_full = None;
                self.energy_full = None;
                return Ok(());
            }
            return Err(BlockError(
                "battery".into(),
                format!(
                    "Power supply device '{}' does not exist",
                    self.device_path.to_string_lossy()
                ),
            ));
        }

        // Read charge_full exactly once, if it exists, units are µAh
        self.charge_full = if self.device_path.join("charge_full").exists() {
            Some(
                read_file("battery", &self.device_path.join("charge_full"))?
                    .parse::<u64>()
                    .block_error("battery", "failed to parse charge_full")?,
            )
        } else {
            None
        };

        // Read energy_full exactly once, if it exists. Units are µWh.
        self.energy_full = if self.device_path.join("energy_full").exists() {
            Some(
                read_file("battery", &self.device_path.join("energy_full"))?
                    .parse::<u64>()
                    .block_error("battery", "failed to parse energy_full")?,
            )
        } else {
            None
        };

        Ok(())
    }

    fn status(&self) -> Result<String> {
        read_file("battery", &self.device_path.join("status"))
    }

    fn capacity(&self) -> Result<u64> {
        let capacity_path = self.device_path.join("capacity");
        let charge_path = self.device_path.join("charge_now");
        let energy_path = self.device_path.join("energy_now");

        let capacity = if capacity_path.exists() {
            read_file("battery", &capacity_path)?
                .parse::<u64>()
                .block_error("battery", "failed to parse capacity")?
        } else if charge_path.exists() && self.charge_full.is_some() {
            let charge = read_file("battery", &charge_path)?
                .parse::<u64>()
                .block_error("battery", "failed to parse charge_now")?;
            ((charge as f64 / self.charge_full.unwrap() as f64) * 100.0) as u64
        } else if energy_path.exists() && self.energy_full.is_some() {
            let charge = read_file("battery", &energy_path)?
                .parse::<u64>()
                .block_error("battery", "failed to parse energy_now")?;
            ((charge as f64 / self.energy_full.unwrap() as f64) * 100.0) as u64
        } else {
            return Err(BlockError(
                "battery".to_string(),
                "Device does not support reading capacity, charge, or energy".to_string(),
            ));
        };

        match capacity {
            0..=100 => Ok(capacity),
            // We need to cap it at 100, because the kernel may report
            // charge_now same as charge_full_design when the battery is full,
            // leading to >100% charge.
            _ => Ok(100),
        }
    }

    fn time_remaining(&self) -> Result<u64> {
        let time_to_empty_now_path = self.device_path.join("time_to_empty_now");
        let time_to_empty = if time_to_empty_now_path.exists() {
            read_file("battery", &time_to_empty_now_path)?
                .parse::<u64>()
                .block_error("battery", "failed to parse time to empty")
        } else {
            Err(BlockError(
                "battery".to_string(),
                "Device does not support reading time to empty directly".to_string(),
            ))
        };
        let time_to_full_now_path = self.device_path.join("time_to_full_now");
        let time_to_full = if time_to_full_now_path.exists() {
            read_file("battery", &time_to_full_now_path)?
                .parse::<u64>()
                .block_error("battery", "failed to parse time to full")
        } else {
            Err(BlockError(
                "battery".to_string(),
                "Device does not support reading time to full directly".to_string(),
            ))
        };

        // Units are µWh
        let full = if self.energy_full.is_some() {
            self.energy_full
        } else if self.charge_full.is_some() {
            self.charge_full
        } else {
            None
        };

        // Units are µWh/µAh
        let energy_path = self.device_path.join("energy_now");
        let charge_path = self.device_path.join("charge_now");
        let fill = if energy_path.exists() {
            read_file("battery", &energy_path)?
                .parse::<f64>()
                .block_error("battery", "failed to parse energy_now")
        } else if charge_path.exists() {
            read_file("battery", &charge_path)?
                .parse::<f64>()
                .block_error("battery", "failed to parse charge_now")
        } else {
            Err(BlockError(
                "battery".to_string(),
                "Device does not support reading energy".to_string(),
            ))
        };

        let power_path = self.device_path.join("power_now");
        let current_path = self.device_path.join("current_now");
        let usage = if power_path.exists() {
            read_file("battery", &power_path)?
                .parse::<f64>()
                .block_error("battery", "failed to parse power_now")
        } else if current_path.exists() {
            read_file("battery", &current_path)?
                .parse::<f64>()
                .block_error("battery", "failed to parse current_now")
        } else {
            Err(BlockError(
                "battery".to_string(),
                "Device does not support reading power".to_string(),
            ))
        };

        // If the device driver uses the combination of energy_full, energy_now and power_now,
        // all values (full, fill, and usage) are in Watts, while if it uses charge_full, charge_now
        // and current_now, they're in Amps. In all 3 equations below the units cancel out and
        // we're left with a time value.
        let status = self.status()?;
        match status.as_str() {
            "Discharging" => {
                if time_to_empty.is_ok() {
                    time_to_empty
                } else if fill.is_ok() && usage.is_ok() {
                    Ok(((fill.unwrap() / usage.unwrap()) * 60.0) as u64)
                } else {
                    Err(BlockError(
                        "battery".to_string(),
                        "Device does not support any method of calculating time to empty"
                            .to_string(),
                    ))
                }
            }
            "Charging" => {
                if time_to_full.is_ok() {
                    time_to_full
                } else if full.is_some() && fill.is_ok() && usage.is_ok() {
                    Ok((((full.unwrap() as f64 - fill.unwrap()) / usage.unwrap()) * 60.0) as u64)
                } else {
                    Err(BlockError(
                        "battery".to_string(),
                        "Device does not support any method of calculating time to full"
                            .to_string(),
                    ))
                }
            }
            _ => {
                // TODO: What should we return in this case? It seems that under
                // some conditions sysfs will return 0 for some readings (energy
                // or power), so perhaps the most natural thing to do is emulate
                // that.
                Ok(0)
            }
        }
    }

    fn power_consumption(&self) -> Result<u64> {
        // power consumption in µWh
        let power_path = self.device_path.join("power_now");
        // current consumption in µA
        let current_path = self.device_path.join("current_now");
        // voltage in µV
        let voltage_path = self.device_path.join("voltage_now");

        if power_path.exists() {
            Ok(read_file("battery", &power_path)?
                .parse::<u64>()
                .block_error("battery", "failed to parse power_now")?)
        } else if current_path.exists() && voltage_path.exists() {
            let current = read_file("battery", &current_path)?
                .parse::<u64>()
                .block_error("battery", "failed to parse current_now")?;
            let voltage = read_file("battery", &voltage_path)?
                .parse::<u64>()
                .block_error("battery", "failed to parse voltage_now")?;
            Ok((current * voltage) / 1_000_000)
        } else {
            Err(BlockError(
                "battery".to_string(),
                "Device does not support power consumption".to_string(),
            ))
        }
    }
}

/// Represents a battery known to apcaccess.
pub struct ApcUpsDevice {
    con: ApcAccess,
    allow_missing: bool,
    status: Option<String>,
    charge_percent: f64,
    time_left: f64,
    nom_power: f64,
    load_percent: f64,
}

impl ApcUpsDevice {
    pub fn from_device(device: &str, allow_missing: bool) -> Result<ApcUpsDevice> {
        Ok(ApcUpsDevice {
            con: ApcAccess::new(device, 1).block_error(
                "battery",
                &format!("Could not create a apcaccess connection to {}", device),
            )?,
            status: None,
            allow_missing,
            charge_percent: 0.0,
            time_left: 0.0,
            nom_power: 0.0,
            load_percent: 0.0,
        })
    }
}

impl BatteryDevice for ApcUpsDevice {
    fn is_available(&self) -> bool {
        self.con.is_available(&self.con.get_status())
    }

    fn refresh_device_info(&mut self) -> Result<()> {
        fn prepare_value(
            status_data: &HashMap<String, String>,
            stat_name: &str,
            required_unit: &str,
        ) -> Result<f64> {
            match status_data.get(stat_name) {
                Some(charge_percent) => {
                    let (value, unit) = charge_percent
                        .split_once(' ')
                        .block_error("battery", &format!("could not split {}", stat_name))
                        .unwrap();
                    if unit == required_unit {
                        return Ok(str::parse::<f64>(value)
                            .block_error(
                                "battery",
                                &format!("could not parse {} to float", stat_name),
                            )
                            .unwrap());
                    } else {
                        return Err(BlockError(
                            "battery".to_string(),
                            format!(
                                "Expected unit for {} are {}, but got {}",
                                stat_name, required_unit, unit
                            ),
                        ));
                    }
                }
                _ => {
                    return Err(BlockError(
                        "battery".to_string(),
                        format!("{} not in apcaccess data", stat_name),
                    ))
                }
            }
        }

        let status_result = self.con.get_status();
        let status_data = self.con.get_status().unwrap_or_default();
        self.status = status_data.get("STATUS").map(String::from);

        if !self.con.is_available(&status_result) {
            // The user indicated that it's ok for this battery to be missing/go away
            if self.allow_missing {
                self.charge_percent = 0.0;
                self.time_left = 0.0;
                self.nom_power = 0.0;
                self.load_percent = 0.0;
                return Ok(());
            }
            return Err(BlockError(
                "battery".into(),
                "Unable to communicate with apcupsd".to_string(),
            ));
        }

        // NOTE: Percentages are 0.0-100.0, not 0.0-1.0
        self.charge_percent = prepare_value(&status_data, "BCHARGE", "Percent")?;
        self.time_left = prepare_value(&status_data, "TIMELEFT", "Minutes")?;
        self.nom_power = prepare_value(&status_data, "NOMPOWER", "Watts")?;
        self.load_percent = prepare_value(&status_data, "LOADPCT", "Percent")?;

        Ok(())
    }

    fn status(&self) -> Result<String> {
        let charge_percent = self.charge_percent;
        if let Some(status) = &self.status {
            if status.contains("ONBATT") {
                if charge_percent == 0.0 {
                    return Ok("Empty".to_string());
                } else {
                    return Ok("Discharging".to_string());
                }
            } else if status.contains("ONLINE") {
                if charge_percent >= 100.0 {
                    return Ok("Full".to_string());
                } else {
                    return Ok("Charging".to_string());
                }
            }
        }
        Ok("Unknown".to_string())
    }

    fn capacity(&self) -> Result<u64> {
        let capacity = self.charge_percent;
        if capacity > 100.0 {
            Ok(100)
        } else {
            Ok(capacity as u64)
        }
    }

    fn time_remaining(&self) -> Result<u64> {
        Ok(self.time_left as u64)
    }

    fn power_consumption(&self) -> Result<u64> {
        //Watts * Percent (0.0-100.0) / 100 * 1_000_000.0 = μW
        //Watts * Percent (0.0-100.0) * 10_000.0 = μW
        Ok((self.nom_power * self.load_percent * 10_000.0) as u64)
    }
}

pub struct UpowerDevice {
    device: String,
    device_path: Arc<Mutex<Option<String>>>,
    con: dbus::ffidisp::Connection,
    allow_missing: bool,
}

impl UpowerDevice {
    /// Create the UPower device from the `device` string, which is converted to
    /// the path `"/org/freedesktop/UPower/devices/<device>"`. Raises an error
    /// if D-Bus does not respond.
    pub fn from_device(device: &str, allow_missing: bool) -> Result<Self> {
        let con = dbus::ffidisp::Connection::new_system()
            .block_error("battery", "Failed to establish D-Bus connection.")?;

        let device_path = UpowerDevice::get_device_path(device, &con)?;

        if device_path.is_some() || allow_missing {
            Ok(UpowerDevice {
                device: device.to_string(),
                device_path: Arc::new(Mutex::new(device_path)),
                con,
                allow_missing,
            })
        } else {
            Err(BlockError(
                "battery".to_string(),
                "UPower device could not be found.".to_string(),
            ))
        }
    }

    /// Monitor UPower property changes in a separate thread and send updates
    /// via the `update_request` channel.
    pub fn monitor(&self, id: usize, update_request: Sender<Task>) {
        let device = self.device.clone();
        let device_path = self.device_path.clone();
        thread::Builder::new()
            .name("battery".into())
            .spawn(move || {
                let con = dbus::ffidisp::Connection::new_system()
                    .expect("Failed to establish D-Bus connection.");
                let enumerate_con = dbus::ffidisp::Connection::new_system()
                    .expect("Failed to establish D-Bus connection.");
                let properties_changed_rule = "type='signal',\
                 interface='org.freedesktop.DBus.Properties',\
                 member='PropertiesChanged'";

                let device_removed_rule = "type='signal',\
                    interface='org.freedesktop.UPower',\
                    member='DeviceRemoved'";
                let device_added_rule = "type='signal',\
                    interface='org.freedesktop.UPower',\
                    member='DeviceAdded'";

                // First we're going to get an (irrelevant) NameAcquired event.
                con.incoming(10_000).next();

                con.add_match(properties_changed_rule)
                    .expect("Failed to add D-Bus match rule.");
                con.add_match(device_removed_rule)
                    .expect("Failed to add D-Bus match rule.");
                con.add_match(device_added_rule)
                    .expect("Failed to add D-Bus match rule.");

                loop {
                    if let Some(msg) = con.incoming(10_000).next() {
                        if let Some(interface) =
                            msg.interface().map(|interface| interface.to_string())
                        {
                            let device_changed = interface == "org.freedesktop.UPower"
                                && msg
                                    .get1::<dbus::Path>()
                                    .expect("Unable to get objectpath argument")
                                    .starts_with("/org/freedesktop/UPower/devices/");

                            // If has been added or removed update the device path
                            if device_changed {
                                *device_path.lock().unwrap() =
                                    UpowerDevice::get_device_path(&device, &enumerate_con).unwrap();
                            }

                            if device_changed || interface == "org.freedesktop.DBus.Properties" {
                                update_request
                                    .send(Task {
                                        id,
                                        update_time: Instant::now(),
                                    })
                                    .unwrap();
                            }
                        }
                    }
                }
            })
            .unwrap();
    }

    // Get a value from the UPower device. If there is a failure in doing so
    // Then either a fallback value is used, if allow_missing is true, or
    // and exception is raised.
    fn get_upower_value<T: for<'b> dbus::arg::Get<'b>>(
        &self,
        key: &str,
        fallback_value: T,
    ) -> Result<T> {
        if let Some(device_path) = &*self.device_path.lock().unwrap() {
            if let Ok(value) = self
                .con
                .with_path("org.freedesktop.UPower", device_path, 1000)
                .get::<T>("org.freedesktop.UPower.Device", key)
            {
                return Ok(value);
            }
        }
        if self.allow_missing {
            return Ok(fallback_value);
        }
        Err(BlockError(
            "battery".into(),
            format!("Failed to read UPower {} property.", key),
        ))
    }

    // Get device path. Raises exception if dbus communication fails.
    // If the device doesn't exist an exception raised unless allow_missing == true
    fn get_device_path(device: &str, con: &dbus::ffidisp::Connection) -> Result<Option<String>> {
        if device == "DisplayDevice" {
            Ok(Some(String::from(
                "/org/freedesktop/UPower/devices/DisplayDevice",
            )))
        } else {
            let msg = dbus::Message::new_method_call(
                "org.freedesktop.UPower",
                "/org/freedesktop/UPower",
                "org.freedesktop.UPower",
                "EnumerateDevices",
            )
            .block_error("battery", "Failed to create DBus message")?;

            let dbus_reply = con
                .send_with_reply_and_block(msg, 2000)
                .block_error("battery", "Failed to retrieve DBus reply")?;

            // EnumerateDevices returns one argument, which is an array of ObjectPaths (not dbus::tree:ObjectPath).
            let mut paths: Array<dbus::Path, _> = dbus_reply
                .get1()
                .block_error("battery", "Failed to read DBus reply")?;

            Ok(paths
                .find(|entry| entry.ends_with(device))
                .map(|path| path.to_string()))
        }
    }
}

impl BatteryDevice for UpowerDevice {
    fn is_available(&self) -> bool {
        self.device_path.lock().unwrap().is_some()
    }

    fn refresh_device_info(&mut self) -> Result<()> {
        let upower_type = self.get_upower_value("Type", 0_u32)?;
        // https://upower.freedesktop.org/docs/Device.html#Device:Type
        // consider any peripheral, UPS and internal battery
        if upower_type == 1 {
            return Err(BlockError(
                "battery".into(),
                "UPower device is not a battery.".into(),
            ));
        }
        Ok(())
    }

    fn status(&self) -> Result<String> {
        self.get_upower_value("State", 0_u32).map(|status| 
        // https://upower.freedesktop.org/docs/Device.html#Device:State
        match status {
            1 => "Charging".to_string(),
            2 => "Discharging".to_string(),
            3 => "Empty".to_string(),
            4 => "Full".to_string(),
            5 => "Not charging".to_string(),
            6 => "Discharging".to_string(),
            _ => "Unknown".to_string(),
        })
    }

    fn capacity(&self) -> Result<u64> {
        self.get_upower_value("Percentage", 0.0).map(|capacity| {
            if capacity > 100.0 {
                100
            } else {
                capacity as u64
            }
        })
    }

    fn time_remaining(&self) -> Result<u64> {
        let property = if self.status()? == "Charging" {
            "TimeToFull"
        } else {
            "TimeToEmpty"
        };

        self.get_upower_value(property, 0_i64)
            .map(|time_to_empty| (time_to_empty / 60) as u64)
    }

    fn power_consumption(&self) -> Result<u64> {
        // FIXME: Might want to make the interface send Watts instead.
        self.get_upower_value("EnergyRate", 0.0)
            .map(|energy_rate| (energy_rate * 1_000_000.0) as u64)
    }
}

/// A block for displaying information about an internal power supply.
pub struct Battery {
    id: usize,
    output: TextWidget,
    update_interval: Duration,
    device: Box<dyn BatteryDevice>,
    format: FormatTemplate,
    full_format: FormatTemplate,
    missing_format: FormatTemplate,
    allow_missing: bool,
    hide_missing: bool,
    driver: BatteryDriver,
    full_threshold: u64,
    good: u64,
    info: u64,
    warning: u64,
    critical: u64,
    fallback_icons: bool,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "lowercase")]
pub enum BatteryDriver {
    ApcAccess,
    Sysfs,
    Upower,
}

impl Default for BatteryDriver {
    fn default() -> Self {
        BatteryDriver::Sysfs
    }
}

/// Configuration for the [`Battery`](./struct.Battery.html) block.
#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct BatteryConfig {
    /// Update interval in seconds
    #[serde(deserialize_with = "deserialize_duration")]
    pub interval: Duration,

    /// The internal power supply device in `/sys/class/power_supply/` to read from.
    pub device: Option<String>,

    /// Format string for displaying battery information.
    /// placeholders: {percentage}, {bar}, {time} and {power}
    pub format: FormatTemplate,

    /// Format string for displaying battery information when battery is full.
    /// placeholders: {percentage}, {bar}, {time} and {power}
    pub full_format: FormatTemplate,

    /// Format string that's displayed if a battery is missing.
    /// placeholders: {percentage}, {bar}, {time} and {power}
    pub missing_format: FormatTemplate,

    /// The "driver" to use for powering the block. One of "apcaccess", "sysfs", or "upower".
    pub driver: BatteryDriver,

    /// The threshold above which the battery is considered full (no time/percentage shown)
    pub full_threshold: u64,

    /// The threshold above which the remaining capacity is shown as good
    pub good: u64,

    /// The threshold below which the remaining capacity is shown as info
    pub info: u64,

    /// The threshold below which the remaining capacity is shown as warning
    pub warning: u64,

    /// The threshold below which the remaining capacity is shown as critical
    pub critical: u64,

    /// If the battery device cannot be found, do not fail and show the block anyway.
    pub allow_missing: bool,

    /// If the battery device cannot be found, completely hide this block.
    pub hide_missing: bool,
}

impl Default for BatteryConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(10),
            device: None,
            format: FormatTemplate::default(),
            full_format: FormatTemplate::default(),
            missing_format: FormatTemplate::default(),
            driver: BatteryDriver::Sysfs,
            full_threshold: 100,
            good: 60,
            info: 60,
            warning: 30,
            critical: 15,
            allow_missing: false,
            hide_missing: false,
        }
    }
}

impl ConfigBlock for Battery {
    type Config = BatteryConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        shared_config: SharedConfig,
        update_request: Sender<Task>,
    ) -> Result<Self> {
        let device_str = match block_config.device {
            Some(d) => d,
            None => match block_config.driver {
                BatteryDriver::ApcAccess => "localhost:3551".to_string(),
                BatteryDriver::Upower => "DisplayDevice".to_string(),
                _ => {
                    let sysfs_dir = read_dir("/sys/class/power_supply").block_error(
                        "battery",
                        "failed to read /sys/class/power_supply direcory",
                    )?;
                    let mut found_battery_devices = Vec::<String>::new();
                    for entry in sysfs_dir {
                        let dir = entry?;
                        if read_to_string(dir.path().join("type"))
                            .map(|t| t.trim() == "Battery")
                            .unwrap_or(false)
                        {
                            found_battery_devices
                                .push(dir.file_name().to_str().unwrap().to_string());
                        }
                    }

                    // Better to default to the system battery, rather than possibly a keyboard or mouse battery.
                    // System batteries usually start with BAT or CMB.
                    // Otherwise, just grab the first one from the list.
                    let chosen_device = if let Some(preferred_device) = found_battery_devices
                        .iter()
                        .find(|&s| s.starts_with("BAT") || s.starts_with("CMB"))
                    {
                        Some(preferred_device)
                    } else {
                        found_battery_devices.first()
                    };

                    match chosen_device {
                        Some(d) => d.to_string(),
                        None => {
                            if block_config.allow_missing {
                                // TODO: If the battery isn't actually BAT0, then even if it appears again we will never update
                                // Need to implement device refresh
                                "BAT0".to_string()
                            } else {
                                return Err(BlockError("battery".to_string(), "failed to determine default battery - please set your battery device in the configuration file".to_string()));
                            }
                        }
                    }
                }
            },
        };

        let device: Box<dyn BatteryDevice> = match block_config.driver {
            BatteryDriver::ApcAccess => Box::new(ApcUpsDevice::from_device(
                &device_str,
                block_config.allow_missing,
            )?),
            BatteryDriver::Upower => {
                let out = UpowerDevice::from_device(&device_str, block_config.allow_missing)?;
                out.monitor(id, update_request);
                Box::new(out)
            }
            BatteryDriver::Sysfs => Box::new(PowerSupplyDevice::from_device(
                &device_str,
                block_config.allow_missing,
            )?),
        };

        let fallback = match shared_config.get_icon("bat_10") {
            Ok(_) => false,
            Err(_) => {
                eprintln!("Icon bat_10 not found in your icons file. Please check NEWS.md");
                true
            }
        };

        Ok(Battery {
            id,
            update_interval: block_config.interval,
            output: TextWidget::new(id, 0, shared_config),
            device,
            format: block_config.format.with_default("{percentage}")?,
            full_format: block_config.full_format.with_default("")?,
            missing_format: block_config.missing_format.with_default("{percentage}")?,
            allow_missing: block_config.allow_missing,
            hide_missing: block_config.hide_missing,
            driver: block_config.driver,
            full_threshold: block_config.full_threshold,
            good: block_config.good,
            info: block_config.info,
            warning: block_config.warning,
            critical: block_config.critical,
            // TODO remove on next release
            fallback_icons: fallback,
        })
    }
}

impl Block for Battery {
    fn update(&mut self) -> Result<Option<Update>> {
        // TODO: Maybe use dbus to immediately signal when the battery state changes.

        // Exit early, if the battery device went missing, but the user
        // allows this device to go missing.
        if !self.device.is_available() && self.allow_missing {
            // Respect the original format string, even if the battery
            // cannot be found right now.
            let values = map!(
                "percentage" => Value::from_string("X".to_string()),
                "time" => Value::from_string("xx:xx".to_string()),
                "power" => Value::from_string("N/A".to_string()),
            );

            self.output.set_icon("bat_not_available")?;
            self.output.set_texts(self.missing_format.render(&values)?);
            self.output.set_state(State::Warning);
        } else {
            // The device may have gone missing
            // It may be a different battery now, thereby refresh the device specs.
            self.device.refresh_device_info()?;

            let status = self.device.status()?;
            let capacity = self.device.capacity();
            let values = map!(
                "percentage" => match capacity {
                    Ok(capacity) => Value::from_integer(capacity as i64).percents(),
                    _ => Value::from_string("×".into()),
                },
                "time" => match self.device.time_remaining() {
                    Ok(0) => Value::from_string("".into()),
                    Ok(time) => Value::from_string(format!("{}:{:02}", std::cmp::min(time / 60, 99), time % 60)),
                    _ => Value::from_string("×".into()),
                },
                // convert µW to W for display
                "power" => match self.device.power_consumption() {
                    Ok(power) => Value::from_float(power as f64 * 1e-6).watts(),
                    _ => Value::from_string("×".into()),
                },
            );

            let capacity_is_above_full_threshold = match capacity {
                Ok(capacity) => (capacity >= self.full_threshold),
                _ => false,
            };

            if status == "Full" || status == "Not charging" || capacity_is_above_full_threshold {
                self.output.set_icon("bat_full")?;
                self.output.set_texts(self.full_format.render(&values)?);
                self.output.set_state(State::Good);
            } else {
                self.output.set_texts(self.format.render(&values)?);

                // Check if the battery is in charging mode and change the state to Good.
                // Otherwise, adjust the state depeding the power percentance.
                match status.as_str() {
                    "Charging" => {
                        self.output.set_state(State::Good);
                    }
                    _ => {
                        self.output.set_state(match capacity {
                            Ok(capacity) => {
                                if capacity <= self.critical {
                                    State::Critical
                                } else if capacity <= self.warning {
                                    State::Warning
                                } else if capacity <= self.info {
                                    State::Info
                                } else if capacity > self.good {
                                    State::Good
                                } else {
                                    State::Idle
                                }
                            }
                            Err(_) => State::Warning,
                        });
                    }
                }

                self.output.set_icon(match status.as_str() {
                    "Discharging" => battery_level_to_icon(capacity, self.fallback_icons),
                    "Charging" => "bat_charging",
                    _ => battery_level_to_icon(capacity, self.fallback_icons),
                })?;
            }
        }

        match self.driver {
            BatteryDriver::ApcAccess | BatteryDriver::Sysfs => {
                Ok(Some(Update::Every(self.update_interval)))
            }
            BatteryDriver::Upower => Ok(None),
        }
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        // Don't display the block at all, if it's configured to be hidden on missing batteries
        if !self.device.is_available() && self.hide_missing {
            return Vec::new();
        }

        vec![&self.output]
    }

    fn id(&self) -> usize {
        self.id
    }
}

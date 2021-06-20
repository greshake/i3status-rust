//! A block for displaying information about an internal power supply.
//!
//! This module contains the [`Battery`](./struct.Battery.html) block, which can
//! display the status, capacity, and time remaining for (dis)charge for an
//! internal power supply.

use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use dbus::arg::Array;
use dbus::ffidisp::stdintf::org_freedesktop_dbus::Properties;
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::SharedConfig;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::formatting::value::Value;
use crate::formatting::FormatTemplate;
use crate::scheduler::Task;
use crate::util::{battery_level_to_icon, read_file};
use crate::widgets::text::TextWidget;
use crate::widgets::{I3BarWidget, Spacing, State};

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

    /// Query the current power consumption, in uW.
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

/// Represents a battery known to UPower.
pub struct UpowerDevice {
    device_path: String,
    con: dbus::ffidisp::Connection,
}

impl UpowerDevice {
    /// Create the UPower device from the `device` string, which is converted to
    /// the path `"/org/freedesktop/UPower/devices/battery_<device>"`, except if
    /// `device` equals `"DisplayDevice"`, in which case it is converted to the
    /// path `"/org/freedesktop/UPower/devices/DisplayDevice"`. Raises an error
    /// if D-Bus cannot connect to this device, or if the device is not a
    /// battery.
    pub fn from_device(device: &str) -> Result<Self> {
        let device_path;
        let con = dbus::ffidisp::Connection::get_private(dbus::ffidisp::BusType::System)
            .block_error("battery", "Failed to establish D-Bus connection.")?;

        if device == "DisplayDevice" {
            device_path = String::from("/org/freedesktop/UPower/devices/DisplayDevice");
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

            device_path = paths
                .find(|entry| entry.ends_with(device))
                .block_error("battery", "UPower device could not be found.")?
                .to_string();
        }
        let upower_type: u32 = con
            .with_path("org.freedesktop.UPower", &device_path, 1000)
            .get("org.freedesktop.UPower.Device", "Type")
            .block_error("battery", "Failed to read UPower Type property.")?;

        // https://upower.freedesktop.org/docs/Device.html#Device:Type
        // consider any peripheral, UPS and internal battery
        if upower_type == 1 {
            return Err(BlockError(
                "battery".into(),
                "UPower device is not a battery.".into(),
            ));
        }
        Ok(UpowerDevice { device_path, con })
    }

    /// Monitor UPower property changes in a separate thread and send updates
    /// via the `update_request` channel.
    pub fn monitor(&self, id: usize, update_request: Sender<Task>) {
        let path = self.device_path.clone();
        thread::Builder::new()
            .name("battery".into())
            .spawn(move || {
                let con = dbus::ffidisp::Connection::get_private(dbus::ffidisp::BusType::System)
                    .expect("Failed to establish D-Bus connection.");
                let rule = format!(
                    "type='signal',\
                 path='{}',\
                 interface='org.freedesktop.DBus.Properties',\
                 member='PropertiesChanged'",
                    path
                );

                // First we're going to get an (irrelevant) NameAcquired event.
                con.incoming(10_000).next();

                con.add_match(&rule)
                    .expect("Failed to add D-Bus match rule.");

                loop {
                    if con.incoming(10_000).next().is_some() {
                        update_request
                            .send(Task {
                                id,
                                update_time: Instant::now(),
                            })
                            .unwrap();
                        // Avoid update spam.
                        // TODO: Is this necessary?
                        thread::sleep(Duration::from_millis(1000))
                    }
                }
            })
            .unwrap();
    }
}

impl BatteryDevice for UpowerDevice {
    fn is_available(&self) -> bool {
        true // TODO: has to be implemented for UPower
    }

    fn refresh_device_info(&mut self) -> Result<()> {
        Ok(())
    }

    fn status(&self) -> Result<String> {
        let status: u32 = self
            .con
            .with_path("org.freedesktop.UPower", &self.device_path, 1000)
            .get("org.freedesktop.UPower.Device", "State")
            .block_error("battery", "Failed to read UPower State property.")?;

        // https://upower.freedesktop.org/docs/Device.html#Device:State
        match status {
            1 => Ok("Charging".to_string()),
            2 => Ok("Discharging".to_string()),
            3 => Ok("Empty".to_string()),
            4 => Ok("Full".to_string()),
            5 => Ok("Not charging".to_string()),
            6 => Ok("Discharging".to_string()),
            _ => Ok("Unknown".to_string()),
        }
    }

    fn capacity(&self) -> Result<u64> {
        let capacity: f64 = self
            .con
            .with_path("org.freedesktop.UPower", &self.device_path, 1000)
            .get("org.freedesktop.UPower.Device", "Percentage")
            .block_error("battery", "Failed to read UPower Percentage property.")?;

        if capacity > 100.0 {
            Ok(100)
        } else {
            Ok(capacity as u64)
        }
    }

    fn time_remaining(&self) -> Result<u64> {
        let property = if self.status()? == "Charging" {
            "TimeToFull"
        } else {
            "TimeToEmpty"
        };
        let time_to_empty: i64 = self
            .con
            .with_path("org.freedesktop.UPower", &self.device_path, 1000)
            .get("org.freedesktop.UPower.Device", property)
            .block_error(
                "battery",
                &format!("Failed to read UPower {} property.", property),
            )?;
        Ok((time_to_empty / 60) as u64)
    }

    fn power_consumption(&self) -> Result<u64> {
        let energy_rate: f64 = self
            .con
            .with_path("org.freedesktop.UPower", &self.device_path, 1000)
            .get("org.freedesktop.UPower.Device", "EnergyRate")
            .block_error("battery", "Failed to read UPower EnergyRate property.")?;
        // FIXME: Might want to make the interface send Watts instead.
        Ok((energy_rate * 1_000_000.0) as u64)
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
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "lowercase")]
pub enum BatteryDriver {
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
    pub device: String,

    /// Format string for displaying battery information.
    /// placeholders: {percentage}, {bar}, {time} and {power}
    pub format: FormatTemplate,

    /// Format string for displaying battery information when battery is full.
    /// placeholders: {percentage}, {bar}, {time} and {power}
    pub full_format: FormatTemplate,

    /// Format string that's displayed if a battery is missing.
    /// placeholders: {percentage}, {bar}, {time} and {power}
    pub missing_format: FormatTemplate,

    /// The "driver" to use for powering the block. One of "sysfs" or "upower".
    pub driver: BatteryDriver,

    /// The threshold above which the battery is considered full (no time/precentage shown)
    pub full_threshold: u64,

    /// The threshold above which the remaining capacity is shown as good
    pub good: u64,

    /// The threshold below which the remaining capacity is shown as info
    pub info: u64,

    /// The threshold below which the remaining capacity is shown as warning
    pub warning: u64,

    /// The threshold below which the remaining capacity is shown as critical
    pub critical: u64,

    /// If the battery device cannot be found, do not fail and show the block anyway (sysfs only).
    pub allow_missing: bool,

    /// If the battery device cannot be found, completely hide this block.
    pub hide_missing: bool,
}

fn default_device() -> String {
    let mut res = "BAT0".to_string();
    let mut found = false;
    if let Ok(dir) = std::fs::read_dir("/sys/class/power_supply") {
        for entry in dir {
            if let Ok(e) = entry {
                if let Some(f) = e.file_name().to_str() {
                    if f.starts_with("BAT") && (!found || f < res.as_str()) {
                        found = true;
                        res = f.to_string();
                    }
                }
            }
        }
    }
    res
}

impl Default for BatteryConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(10),
            device: default_device(),
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
        let device: Box<dyn BatteryDevice> = match block_config.driver {
            BatteryDriver::Upower => {
                let out = UpowerDevice::from_device(&block_config.device)?;
                out.monitor(id, update_request);
                Box::new(out)
            }
            BatteryDriver::Sysfs => Box::new(PowerSupplyDevice::from_device(
                &block_config.device,
                block_config.allow_missing,
            )?),
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

            return match self.driver {
                BatteryDriver::Sysfs => Ok(Some(Update::Every(self.update_interval))),
                BatteryDriver::Upower => Ok(None),
            };
        }

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
            self.output.set_spacing(Spacing::Hidden);
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
                "Discharging" => battery_level_to_icon(capacity),
                "Charging" => "bat_charging",
                _ => battery_level_to_icon(capacity),
            })?;
            self.output.set_spacing(Spacing::Normal);
        }

        match self.driver {
            BatteryDriver::Sysfs => Ok(Some(self.update_interval.into())),
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

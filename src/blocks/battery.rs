//! A block for displaying information about an internal power supply.
//!
//! This module contains the [`Battery`](./struct.Battery.html) block, which can
//! display the status, capacity, and time remaining for (dis)charge for an
//! internal power supply.

use std::path::{Path, PathBuf};
use util::FormatTemplate;
use std::time::{Duration, Instant};
use std::thread;

use chan::Sender;
use uuid::Uuid;

use block::{Block, ConfigBlock};
use blocks::dbus;
use blocks::dbus::stdintf::org_freedesktop_dbus::Properties;
use config::Config;
use de::deserialize_duration;
use errors::*;
use scheduler::Task;
use util::read_file;
use widget::{I3BarWidget, State};
use widgets::text::TextWidget;

/// A battery device can be queried for a few properties relevant to the user.
pub trait BatteryDevice {
    /// Query the device status, one of `"Full"`, `"Charging"`, `"Discharging"`,
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
    charge_full: Option<u64>,
    energy_full: Option<u64>,
}

impl PowerSupplyDevice {
    /// Use the power supply device `device`, as found in the
    /// `/sys/class/power_supply` directory. Raises an error if a directory for
    /// that device is not found.
    pub fn from_device(device: &str) -> Result<Self> {
        let device_path = Path::new("/sys/class/power_supply").join(device);
        if !device_path.exists() {
            return Err(BlockError(
                "battery".to_string(),
                format!(
                    "Power supply device '{}' does not exist",
                    device_path.to_string_lossy()
                ),
            ));
        }

        // Read charge_full exactly once, if it exists.
        let charge_full = if device_path.join("charge_full").exists() {
            Some(read_file("battery", &device_path.join("charge_full"))?
                .parse::<u64>()
                .block_error("battery", "failed to parse charge_full")?)
        } else {
            None
        };

        // Read energy_full exactly once, if it exists.
        let energy_full = if device_path.join("energy_full").exists() {
            Some(read_file("battery", &device_path.join("energy_full"))?
                .parse::<u64>()
                .block_error("battery", "failed to parse energy_full")?)
        } else {
            None
        };

        Ok(PowerSupplyDevice {
            device_path,
            charge_full,
            energy_full,
        })
    }
}

impl BatteryDevice for PowerSupplyDevice {
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
            0...100 => Ok(capacity),
            // We need to cap it at 100, because the kernel may report
            // charge_now same as charge_full_design when the battery is full,
            // leading to >100% charge.
            _ => Ok(100),
        }
    }

    fn time_remaining(&self) -> Result<u64> {
        let full = if self.energy_full.is_some() {
            self.energy_full.unwrap()
        } else if self.charge_full.is_some() {
            self.charge_full.unwrap()
        } else {
            return Err(BlockError(
                "battery".to_string(),
                "Device does not support reading energy".to_string(),
            ))
        };

        let energy_path = self.device_path.join("energy_now");
        let charge_path = self.device_path.join("charge_now");
        let fill = if energy_path.exists() {
            read_file("battery", &energy_path)?
                .parse::<f64>()
                .block_error("battery", "failed to parse energy_now")?
        } else if charge_path.exists() {
            read_file("battery", &charge_path)?
                .parse::<f64>()
                .block_error("battery", "failed to parse charge_now")?
        } else {
            return Err(BlockError(
                "battery".to_string(),
                "Device does not support reading energy".to_string(),
            ));
        };

        let power_path = self.device_path.join("power_now");
        let current_path = self.device_path.join("current_now");
        let usage = if power_path.exists() {
            read_file("battery", &power_path)?
                .parse::<f64>()
                .block_error("battery", "failed to parse power_now")?
        } else if current_path.exists() {
            read_file("battery", &current_path)?
                .parse::<f64>()
                .block_error("battery", "failed to parse current_now")?
        } else {
            return Err(BlockError(
                "battery".to_string(),
                "Device does not support reading power".to_string(),
            ));
        };

        let status = self.status()?;
        match status.as_str() {
            "Full" => Ok(((full as f64 / usage) * 60.0) as u64),
            "Discharging" => Ok(((fill / usage) * 60.0) as u64),
            "Charging" => Ok((((full as f64 - fill) / usage) * 60.0) as u64),
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
        let power_path = self.device_path.join("power_now");

        if power_path.exists() {
            Ok(read_file("battery", &power_path)?
                .parse::<u64>()
                .block_error("battery", "failed to parse power_now")?)
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
    con: dbus::Connection,
}

impl UpowerDevice {
    /// Create the UPower device from the `device` string, which is converted to
    /// the path `"/org/freedesktop/UPower/devices/battery_<device>"`, except if
    /// `device` equals `"DisplayDevice"`, in which case it is converted to the
    /// path `"/org/freedesktop/UPower/devices/DisplayDevice"`. Raises an error
    /// if D-Bus cannot connect to this device, or if the device is not a
    /// battery.
    pub fn from_device(device: &str) -> Result<Self> {
        let device_name = if device == "DisplayDevice" { device.to_string() } else { format!("battery_{}", device) };
        let device_path = format!("/org/freedesktop/UPower/devices/{}", device_name);
        let con = dbus::Connection::get_private(dbus::BusType::System)
            .block_error("battery", "Failed to establish D-Bus connection.")?;

        let upower_type: u32 = con
            .with_path("org.freedesktop.UPower", &device_path, 1000)
            .get("org.freedesktop.UPower.Device", "Type")
            .block_error("battery", "Failed to read UPower Type property.")?;

        // https://upower.freedesktop.org/docs/Device.html#Device:Type
        if upower_type != 2 {
            return Err(BlockError(
                "battery".into(),
                "UPower device is not a battery.".into(),
            ));
        }
        Ok(UpowerDevice {
            device_path,
            con,
        })
    }

    /// Monitor UPower property changes in a separate thread and send updates
    /// via the `update_request` channel.
    pub fn monitor(&self, id: String, update_request: Sender<Task>) {
        let path = self.device_path.clone();
        thread::spawn(move || {
            let con = dbus::Connection::get_private(dbus::BusType::System)
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
                    update_request.send(Task {
                        id: id.clone(),
                        update_time: Instant::now(),
                    });
                    // Avoid update spam.
                    // TODO: Is this necessary?
                    thread::sleep(Duration::from_millis(1000))
                }
            }
        });
    }
}

impl BatteryDevice for UpowerDevice {
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
    output: TextWidget,
    id: String,
    update_interval: Duration,
    device: Box<BatteryDevice>,
    format: FormatTemplate,
    upower: bool,
}

/// Configuration for the [`Battery`](./struct.Battery.html) block.
#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct BatteryConfig {
    /// Update interval in seconds
    #[serde(default = "BatteryConfig::default_interval", deserialize_with = "deserialize_duration")]
    pub interval: Duration,

    /// The internal power supply device in `/sys/class/power_supply/` to read
    /// from.
    #[serde(default = "BatteryConfig::default_device")]
    pub device: String,

    /// (DEPRECATED) Options for displaying battery information.
    #[serde()]
    pub show: Option<String>,

    /// Format string for displaying battery information.
    /// placeholders: {percentage}, {time} and {power}
    #[serde(default = "BatteryConfig::default_format")]
    pub format: String,

    /// Use UPower to monitor battery status and events.
    #[serde(default = "BatteryConfig::default_upower")]
    pub upower: bool,
}

impl BatteryConfig {
    fn default_interval() -> Duration {
        Duration::from_secs(10)
    }

    fn default_device() -> String {
        "BAT0".to_string()
    }

    fn default_format() -> String {
        "{percentage}%".into()
    }

    fn default_upower() -> bool {
        false
    }
}

impl ConfigBlock for Battery {
    type Config = BatteryConfig;

    fn new(block_config: Self::Config, config: Config, update_request: Sender<Task>) -> Result<Self> {
        // TODO: remove deprecated show types eventually
        let format = match block_config.show {
            Some(show) => match show.as_ref() {
                "time" => "{time}".into(),
                "percentage" => "{percentage}%".into(),
                "both" => "{percentage}% {time}".into(),
                _ => {
                    return Err(BlockError(
                        "battery".into(),
                        "Unknown show option".into(),
                    ));
                }
            },
            None => block_config.format
        };

        let id = Uuid::new_v4().simple().to_string();
        let device: Box<BatteryDevice> = if block_config.upower {
            let out = UpowerDevice::from_device(&block_config.device)?;
            out.monitor(id.clone(), update_request);
            Box::new(out)
        } else {
            Box::new(PowerSupplyDevice::from_device(&block_config.device)?)

        };

        Ok(Battery {
            id,
            update_interval: block_config.interval,
            output: TextWidget::new(config),
            device,
            format: FormatTemplate::from_string(&format)?,
            upower: block_config.upower,
        })
    }
}

impl Block for Battery {
    fn update(&mut self) -> Result<Option<Duration>> {
        // TODO: Maybe use dbus to immediately signal when the battery state changes.

        let status = self.device.status()?;

        if status == "Full" || status == "Not charging" {
            self.output.set_icon("bat_full");
            self.output.set_text("".to_string());
            self.output.set_state(State::Good);
        } else {
            let capacity = self.device.capacity();
            let percentage = match capacity {
                Ok(capacity) => format!("{}", capacity),
                Err(_) => "×".into(),
            };
            let time = match self.device.time_remaining() {
                Ok(time) => format!("{}:{:02}", time / 60, time % 60),
                Err(_) => "×".into(),
            };
            let power = match self.device.power_consumption() {
                Ok(power) => format!("{:.2}", power as f64 / 1000.0 / 1000.0),
                Err(_) => "×".into(),
            };
            let values = map!("{percentage}" => percentage,
                              "{time}" => time,
                              "{power}" => power);
            self.output.set_text(self.format.render_static_str(&values)?);

            // Check if the battery is in charging mode and change the state to Good.
            // Otherwise, adjust the state depeding the power percentance.
            match status.as_str() {
                "Charging" => { self.output.set_state(State::Good); },
                _ =>
                    { self.output.set_state(match capacity {
                    Ok(0...15) => State::Critical,
                    Ok(16...30) => State::Warning,
                    Ok(31...60) => State::Info,
                    Ok(61...100) => State::Good,
                    _ => State::Warning,
                    });
                }
            }

            self.output.set_icon(match status.as_str() {
                "Discharging" => "bat_discharging",
                "Charging" => "bat_charging",
                _ => "bat",
            });
        }

        if self.upower {
            Ok(None)
        } else {
            Ok(Some(self.update_interval))
        }
    }

    fn view(&self) -> Vec<&I3BarWidget> {
        vec![&self.output]
    }

    fn id(&self) -> &str {
        &self.id
    }
}

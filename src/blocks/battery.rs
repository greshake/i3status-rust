use std::path::{Path, PathBuf};
use std::time::Duration;

use chan::Sender;
use uuid::Uuid;

use block::{Block, ConfigBlock};
use config::Config;
use de::deserialize_duration;
use errors::*;
use scheduler::Task;
use util::read_file;
use widget::{I3BarWidget, State};
use widgets::text::TextWidget;

/// Represents a physical power supply device.
pub struct PowerSupplyDevice {
    device_path: PathBuf,
    charge_full: Option<u64>,
    energy_full: Option<u64>,
}

impl PowerSupplyDevice {
    /// Use the power supply device `device`. Raises an error if a directory for
    /// that device is not found.
    pub fn from_device(device: String) -> Result<Self> {
        let device_path = Path::new("/sys/class/power_supply").join(device.clone());
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
            device_path: device_path,
            charge_full: charge_full,
            energy_full: energy_full,
        })
    }

    /// Query the device status.
    pub fn status(&self) -> Result<String> {
        read_file("battery", &self.device_path.join("status"))
    }

    /// Query the device's current capacity, as a percent.
    pub fn capacity(&self) -> Result<u64> {
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

    /// Query the estimated time remaining, in minutes, before (dis)charging is
    /// complete.
    pub fn time_remaining(&self) -> Result<u64> {
        let energy_full = match self.energy_full {
            Some(val) => val,
            None => {
                return Err(BlockError(
                    "battery".to_string(),
                    "Device does not support reading energy".to_string(),
                ))
            }
        };
        let energy_path = self.device_path.join("energy_now");
        let energy_now = if energy_path.exists() {
            read_file("battery", &energy_path)?
                .parse::<f64>()
                .block_error("battery", "failed to parse energy_now")?
        } else {
            return Err(BlockError(
                "battery".to_string(),
                "Device does not support reading energy".to_string(),
            ));
        };
        let power_path = self.device_path.join("power_now");
        let power_now = if power_path.exists() {
            read_file("battery", &power_path)?
                .parse::<f64>()
                .block_error("battery", "failed to parse power_now")?
        } else {
            return Err(BlockError(
                "battery".to_string(),
                "Device does not support reading power".to_string(),
            ));
        };
        let status = self.status()?;
        match status.as_str() {
            "Full" => Ok(((energy_full as f64 / power_now) * 60.0) as u64),
            "Discharging" => Ok(((energy_now / power_now) * 60.0) as u64),
            "Charging" => Ok((((energy_full as f64 - energy_now) / power_now) * 60.0) as u64),
            _ => {
                // TODO: What should we return in this case? It seems that under
                // some conditions sysfs will return 0 for some readings (energy
                // or power), so perhaps the most natural thing to do is emulate
                // that.
                Ok(0)
            }
        }
    }
}

pub struct Battery {
    output: TextWidget,
    id: String,
    update_interval: Duration,
    device: PowerSupplyDevice,
    show: ShowType,
}

#[derive(Deserialize, Copy, Clone, Debug)]
#[serde(rename_all = "lowercase")]
pub enum ShowType {
    Time,
    Percentage,
    Both,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct BatteryConfig {
    /// Update interval in seconds
    #[serde(default = "BatteryConfig::default_interval", deserialize_with = "deserialize_duration")]
    pub interval: Duration,

    /// Which BAT device in /sys/class/power_supply/ to read from.
    #[serde(default = "BatteryConfig::default_device")]
    pub device: String,

    /// Show only percentage, time until (dis)charged or both
    #[serde(default = "BatteryConfig::default_show")]
    pub show: ShowType,
}

impl BatteryConfig {
    fn default_interval() -> Duration {
        Duration::from_secs(10)
    }

    fn default_device() -> String {
        "BAT0".to_string()
    }

    fn default_show() -> ShowType {
        ShowType::Percentage
    }
}

impl ConfigBlock for Battery {
    type Config = BatteryConfig;

    fn new(block_config: Self::Config, config: Config, _tx_update_request: Sender<Task>) -> Result<Self> {
        Ok(Battery {
            id: Uuid::new_v4().simple().to_string(),
            update_interval: block_config.interval,
            output: TextWidget::new(config),
            device: try!(PowerSupplyDevice::from_device(block_config.device)),
            show: block_config.show,
        })
    }
}

impl Block for Battery {
    fn update(&mut self) -> Result<Option<Duration>> {
        // TODO: Maybe use dbus to immediately signal when the battery state changes.

        let current_percentage = self.device.capacity()?;

        let state = self.device.status()?;

        let time_remaining = self.device.time_remaining()?;
        let hours = time_remaining / 60;
        let minutes = time_remaining % 60;

        // Don't need to display a percentage when the battery is full
        if current_percentage != 100 && state != "Full" {
            match self.show {
                ShowType::Both => self.output
                    .set_text(format!("{}% {}:{:02}", current_percentage, hours, minutes)),
                ShowType::Percentage => self.output.set_text(format!("{}%", current_percentage)),
                ShowType::Time => self.output.set_text(format!("{}:{:02}", hours, minutes)),
            }
        } else {
            self.output.set_text(String::from(""));
        }

        self.output.set_icon(match state.as_str() {
            "Full" => "bat_full",
            "Discharging" => "bat_discharging",
            "Charging" => "bat_charging",
            _ => "bat",
        });

        self.output.set_state(match current_percentage {
            0...15 => State::Critical,
            16...30 => State::Warning,
            31...60 => State::Info,
            _ => State::Good,
        });

        Ok(Some(self.update_interval))
    }

    fn view(&self) -> Vec<&I3BarWidget> {
        vec![&self.output]
    }

    fn id(&self) -> &str {
        &self.id
    }
}

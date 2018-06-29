use block::{Block, ConfigBlock};
use blocks::lib::*;
use chan::Sender;
use config::Config;
use de::deserialize_duration;
use errors::*;
use scheduler::Task;
use std::time::Duration;
use uuid::Uuid;
use widget::{I3BarWidget, State};
use widgets::text::TextWidget;

pub struct Battery {
    output: TextWidget,
    id: String,
    max_charge: u64,
    update_interval: Duration,
    devices: String,
    show: ShowType,
}

#[derive(PartialEq, Eq)]
enum DeviceState {
    Charging,
    Neutral,
    Discharging,
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
            max_charge: 0,
            update_interval: block_config.interval,
            output: TextWidget::new(config),
            devices: block_config.device,
            show: block_config.show,
        })
    }
}
impl Block for Battery {
    #![cfg_attr(feature = "cargo-clippy", allow(cyclomatic_complexity))]
    fn update(&mut self) -> Result<Option<Duration>> {
        // TODO: Maybe use dbus to immediately signal when the battery state changes.

        struct BatteryData {
            // collect basic data of a battery
            current_percentage: u64,
            max_charge: u64,
            current_charge: u64,
            energy_now: u64,
            energy_full: u64,
            power_now: u64,
            state: String,
        }

        // decode the list of battery devices from status file and get their paths
        let batteries: Vec<String> = self.devices
            .clone()
            .trim()
            .split(',')
            .map(|battery_device| format!("/sys/class/power_supply/{}/", battery_device.trim()))
            .collect();

        // use this to collect data of all the batteries that got passed via config
        let mut batteries_info: Vec<BatteryData> = Vec::new();

        for device_path in &batteries {
            // iterate over available batteries and gather stats for each
            let mut current_percentage: u64;
            let mut max_charge: u64 = 0;
            let mut current_charge: u64 = 0;
            let mut energy_now: u64;
            let mut energy_full: u64;
            let mut power_now: u64;

            if file_exists(&format!("{}capacity", device_path)) {
                current_percentage = match read_file("battery", &format!("{}capacity", device_path))?
                    .parse::<u64>()
                    .block_error("battery", "failed to parse capacity")?
                {
                    capacity if capacity < 100 => capacity,
                    _ => 100,
                }
            } else if file_exists(&format!("{}charge_full", device_path)) && file_exists(&format!("{}charge_now", device_path)) {
                // We only need to read max_charge once, shouldn't change
                if max_charge == 0 {
                    max_charge = read_file("battery", &format!("{}charge_full", device_path))?
                        .parse::<u64>()
                        .block_error("battery", "failed to parse charge_full")?;
                }

                current_charge = read_file("battery", &format!("{}charge_now", device_path))?
                    .parse::<u64>()
                    .block_error("battery", "failed to parse charge_now")?;
                current_percentage = ((current_charge as f64 / self.max_charge as f64) * 100.0) as u64;
                current_percentage = match current_percentage {
                    0...100 => current_percentage,
                    // We need to cap it at 100, because the kernel may report
                    // charge_now same as charge_full_design when the battery
                    // is full, leading to >100% charge.
                    _ => 100,
                };
            } else if file_exists(&format!("{}energy_full", device_path)) && file_exists(&format!("{}energy_now", device_path)) {
                // We only need to read max_charge once, shouldn't change
                if max_charge == 0 {
                    max_charge = read_file("battery", &format!("{}energy_full", device_path))?
                        .parse::<u64>()
                        .block_error("battery", "failed to parse energy_full")?;
                }

                current_charge = read_file("battery", &format!("{}energy_now", device_path))?
                    .parse::<u64>()
                    .block_error("battery", "failed to parse energy_now")?;
                current_percentage = ((current_charge as f64 / self.max_charge as f64) * 100.0) as u64;
                current_percentage = match current_percentage {
                    0...100 => current_percentage,
                    // We need to cap it at 100, because the kernel may report
                    // charge_now same as charge_full_design when the battery
                    // is full, leading to >100% charge.
                    _ => 100,
                };
            } else {
                return Err(BlockError("battery".to_string(), "Device does not support reading capacity, charge or energy".to_string()));
            }

            let state: String = read_file("battery", &format!("{}status", device_path))?;

            energy_now = if file_exists(&format!("{}energy_now", device_path)) {
                read_file("battery", &format!("{}energy_now", device_path))?
                    .parse::<u64>()
                    .block_error("battery", "failed to parse  energy_now")?
            } else {
                0
            };

            energy_full = if file_exists(&format!("{}energy_full", device_path)) {
                read_file("battery", &format!("{}energy_full", device_path))?
                    .parse::<u64>()
                    .block_error("battery", "failed to parse  energy_full")?
            } else {
                0
            };

            power_now = if file_exists(&format!("{}power_now", device_path)) {
                read_file("battery", &format!("{}power_now", device_path))?
                    .parse::<u64>()
                    .block_error("battery", "failed to parse current voltage")?
            } else {
                0
            };

            let battery_summary = BatteryData {
                current_percentage,
                current_charge,
                energy_now,
                energy_full,
                max_charge,
                power_now,
                state,
            };

            batteries_info.push(battery_summary);
        } // done iterating over all batteries and collecting info

        // We can now calculate the stats now.
        // If a system has several batteries, we can calculate the full expected
        // runtime of the device taking charge of all batteries into account

        let mut total_current_charge = 0;
        let mut total_energy_now = 0;
        let mut total_energy_full = 0;
        let mut total_max_charge = 0;
        let mut total_power_now = 0;
        let mut state: i64 = 0;
        // use this to print percentages of each battery in the bar:
        let mut percentages = String::new();

        for battery in batteries_info {
            total_current_charge += battery.current_charge;
            total_energy_now += battery.energy_now;
            total_energy_full += battery.energy_full;
            total_max_charge += battery.max_charge;
            total_power_now += battery.power_now;
            // collect battery percentages
            percentages.push_str(&format!("{}% ", battery.current_percentage));
            // check if we have more batteries charging or discharging
            // if state is negative, most batteries are discharging, positive if more are charging
            if battery.state == "Charging" {
                state += 1;
            } else if battery.state == "Discharging" {
                state -= 1;
            } else if battery.state == "Unknown" {
                // a battery may be Unknown when it's just sitting there not doing anything while the
                // other one is being (dis/)charged
                state += 0;
            }
        }

        let device_state: DeviceState = match state {
            state if state > 0 => DeviceState::Charging,
            state if state < 0 => DeviceState::Discharging,
            _ => DeviceState::Neutral,
        };

        percentages.trim(); // trim excess whitespaces from percentage string

        // also calculate the total percentage for coloring the block accordingly
        let total_percentage = ((total_current_charge as f64 / total_max_charge as f64) * 100.0) as u64;

        // these calculations might get wrong if one battery charges while one discharges
        // however I have not seen this yet, so let's just assume this does not happen and both
        // batteries are are always in similar state (one charging or discharging and others idle/unknown)

        let (hours, minutes) = if total_power_now > 0 && total_energy_now > 0 {
            if device_state == DeviceState::Discharging {
                let h = (total_energy_now as f64 / total_power_now as f64) as u64;
                let m = (((total_energy_now as f64 / total_power_now as f64) - h as f64) * 60.0) as u64;
                (h, m)
            } else if device_state == DeviceState::Charging || device_state == DeviceState::Neutral {
                let h = ((total_energy_full as f64 - total_energy_now as f64) / total_power_now as f64) as u64;
                let m = ((((total_energy_full as f64 - total_energy_now as f64) / total_power_now as f64) - h as f64) * 60.0) as u64;
                (h, m)
            } else {
                (0, 0)
            }
        } else {
            (0, 0)
        };

        let system_looks_charged: bool = total_energy_now >= total_energy_full;

        // Only display percentages if system is charging or discharging
        if device_state != DeviceState::Neutral || system_looks_charged {
            // at least one battery is charging or discharging
            // this check also does not work if one battery charges while another one discharges
            match self.show {
                ShowType::Both => self.output.set_text(format!("{} {}:{:02}", percentages, hours, minutes)),
                ShowType::Percentage => self.output.set_text(percentages),
                ShowType::Time => self.output.set_text(format!("{}:{:02}", hours, minutes)),
            }
        } else {
            self.output.set_text(String::from(""));
        }

        match device_state {
            DeviceState::Neutral => {
                self.output.set_icon("bat_full");
            }
            DeviceState::Charging => {
                self.output.set_icon("bat_charging");
            }
            DeviceState::Discharging => {
                self.output.set_icon("bat_discharging");
            }
        }
        // override if we can see that the system is fully charged
        if system_looks_charged {
            self.output.set_icon("bat_full");
        }

        self.output.set_state(match total_percentage {
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

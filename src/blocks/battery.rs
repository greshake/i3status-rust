use std::time::Duration;
use std::sync::mpsc::Sender;
use scheduler::Task;

use block::{Block, ConfigBlock};
use config::Config;
use de::deserialize_duration;
use errors::*;
use widgets::text::TextWidget;
use widget::{I3BarWidget, State};
use std::fs::OpenOptions;
use std::io::prelude::*;

use uuid::Uuid;

//TODO: Add remaining time
pub struct Battery {
    output: TextWidget,
    id: String,
    max_charge: u64,
    update_interval: Duration,
    device_path: String,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct BatteryConfig {
    /// Update interval in seconds
    #[serde(default = "BatteryConfig::default_interval", deserialize_with = "deserialize_duration")]
    pub interval: Duration,

    /// Which BAT device in /sys/class/power_supply/ to read from.
    #[serde(default = "BatteryConfig::default_device")]
    pub device: usize,
}

impl BatteryConfig {
    fn default_interval() -> Duration {
        Duration::from_secs(10)
    }

    fn default_device() -> usize {
        0
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
            device_path: format!("/sys/class/power_supply/BAT{}/", block_config.device),
        })
    }
}

fn read_file(path: &str) -> Result<String> {
    let mut f = OpenOptions::new()
        .read(true)
        .open(path)
        .block_error("battery", &format!("failed to open file {}", path))?;
    let mut content = String::new();
    f.read_to_string(&mut content)
        .block_error("battery", &format!("failed to read {}", path))?;
    // Removes trailing newline
    content.pop();
    Ok(content)
}

impl Block for Battery {
    fn update(&mut self) -> Result<Option<Duration>> {
        // TODO: Check if charge_ always contains the right values, might be energy_ depending on firmware

        // TODO: Maybe use dbus to immediately signal when the battery state changes.

        // We only need to read max_charge once, shouldn't change
        if self.max_charge == 0 {
            self.max_charge = read_file(&format!("{}charge_full", self.device_path))?
                .parse::<u64>()
                .block_error("battery", "failed to parse charge_full")?;
        }

        let current_charge = read_file(&format!("{}charge_now", self.device_path))?
            .parse::<u64>()
            .block_error("battery", "failed to parse charge_now")?;
        let current_percentage = ((current_charge as f64 / self.max_charge as f64) * 100.0) as u64;
        let current_percentage = match current_percentage {
            0...100 => current_percentage,
            // We need to cap it at 100, because the kernel may report
            // charge_now same as charge_full_design when the battery
            // is full, leading to >100% charge.
            _ => 100,
        };

        let state = read_file(&format!("{}status", self.device_path))?;

        // Don't need to display a percentage when the battery is full
        if current_percentage != 100 && state != "Full" {
            self.output.set_text(format!("{}%", current_percentage));
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

use std::time::Duration;

use block::Block;
use widgets::text::TextWidget;
use widget::{I3BarWidget, State};
use input::I3barEvent;
use std::fs::OpenOptions;
use std::io::prelude::*;

use serde_json::Value;
use uuid::Uuid;

//TODO: Add remaining time
pub struct Battery {
    output: TextWidget,
    id: String,
    max_charge: u64,
    update_interval: Duration,
    device_path: String
}

impl Battery {
    pub fn new(config: Value, theme: Value) -> Battery {
        {
            Battery {
                id: Uuid::new_v4().simple().to_string(),
                max_charge: 0,
                update_interval: Duration::new(get_u64_default!(config, "interval", 10), 0),
                output: TextWidget::new(theme),
                device_path: format!("/sys/class/power_supply/BAT{}/", get_u64_default!(config, "device", 0)),
            }
        }
        
    }
}

fn read_file(path: &str) -> String {
    let mut f = OpenOptions::new()
        .read(true)
        .open(path)
        .expect(&format!("Your system does not support reading {}", path));
    let mut content = String::new();
    f.read_to_string(&mut content).expect(&format!("Failed to read {}", path));
    // Removes trailing newline
    content.pop();
    content
}

impl Block for Battery
{
    fn update(&mut self) -> Option<Duration> {
        // TODO: Check if charge_ always contains the right values, might be energy_ depending on firmware

        // TODO: Maybe use dbus to immediately signal when the battery state changes.

        // We only need to read max_charge once, shouldn't change
        if self.max_charge == 0 {
            self.max_charge = read_file(&format!("{}charge_full", self.device_path)).parse::<u64>().unwrap();
        }

        let current_charge = read_file(&format!("{}charge_now", self.device_path)).parse::<u64>().unwrap();
        let current_percentage = ((current_charge as f64 / self.max_charge as f64) * 100.) as u64;
        let current_percentage = match current_percentage {
            0 ... 100 => current_percentage,
            // We need to cap it at 100, because the kernel may report
            // charge_now same as charge_full_design when the battery
            // is full, leading to >100% charge.
            _ => 100
        };

        let state = read_file(&format!("{}status", self.device_path));

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
            _ => "bat"
        });

        self.output.set_state(match current_percentage {
            0 ... 15 => State::Critical,
            15 ... 30 => State::Warning,
            30 ... 60 => State::Info,
            _ => State::Good
        });

        Some(self.update_interval.clone())
    }
    fn view(&self) -> Vec<&I3BarWidget> {
        vec![&self.output]
    }
    fn click_left(&mut self, _: &I3barEvent) {}
    fn id(&self) -> &str {
        &self.id
    }
}

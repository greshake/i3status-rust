use std::process::Command;
use std::time::Duration;

use crossbeam_channel::Sender;
use serde_derive::Deserialize;
use uuid::Uuid;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::{Config, LogicalDirection};
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::input::I3BarEvent;
use crate::scheduler::Task;
use crate::widget::I3BarWidget;
use crate::widgets::button::ButtonWidget;
use crate::util::has_command;

pub struct Hueshift {
    text: ButtonWidget,
    id: String,
    update_interval: Duration,
    step: u16,
    current_temp: u16,
    max_temp: u16,
    min_temp: u16,
    hue_shifter: Option<String>,

    //useful, but optional
    #[allow(dead_code)]
    config: Config,
    #[allow(dead_code)]
    tx_update_request: Sender<Task>,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct HueshiftConfig {
    /// Update interval in seconds
    #[serde(
        default = "HueshiftConfig::default_interval",
        deserialize_with = "deserialize_duration"
    )]
    pub interval: Duration,

    #[serde(default = "HueshiftConfig::default_max_temp")]
    pub max_temp: u16,
    #[serde(default = "HueshiftConfig::default_min_temp")]
    pub min_temp: u16,

    /// Currently defined temperature default to 6500K
    #[serde(default = "HueshiftConfig::default_current_temp")]
    pub current_temp: u16,
    /// To be set by user as an option

    #[serde(default = "HueshiftConfig::default_hue_shifter")]
    pub hue_shifter: Option<String>,

    /// Default to 100K
    #[serde(default = "HueshiftConfig::default_step")]
    pub step: u16,
}

impl HueshiftConfig {
    fn default_interval() -> Duration {
        Duration::from_secs(5)
    }

    /// Default current temp for any screens
    fn default_current_temp() -> u16 {
        6500 as u16
    }
    /// Max/Min hue temperature (min 1000K, max 10_000K)
    // TODO: Try to detect if we're using redshift or not
    // to set default max_temp either to 10_000K to 25_000K
    fn default_min_temp() -> u16 {
        1000 as u16
    }
    fn default_max_temp() -> u16 {
        10_000 as u16
    }

    fn default_step() -> u16 {
        100 as u16
    }

    fn default_hue_shifter() -> Option<String> {
        let (_redshift, _sct) = what_is_supported();
        // Prefer redshift over sct
        match _redshift {
            Ok(_redshift) => Some(String::from("redshift")),
            Err(_redshift) => 
            match _sct {
                Ok(_sct) => Some(String::from("sct")),
                Err(_sct) => None
            }
        }
    }
}

impl ConfigBlock for Hueshift {
    type Config = HueshiftConfig;

    fn new(
        block_config: Self::Config,
        config: Config,
        tx_update_request: Sender<Task>,
    ) -> Result<Self> {
        let current_temp = block_config.current_temp;
        let mut step = block_config.step; 
        let id = Uuid::new_v4().to_simple().to_string();
        // limit too big steps at 2000K to avoid too brutal changes
        if step > 10_000 {
            step = 2000;
        }
        Ok(Hueshift {
            id: id.clone(),
            update_interval: block_config.interval,
            text: ButtonWidget::new(config.clone(), &id).with_text(&current_temp.to_string()),
            tx_update_request,
            step: step,
            max_temp: block_config.max_temp,
            min_temp: block_config.min_temp,
            current_temp: block_config.current_temp,
            hue_shifter: block_config.hue_shifter,
            config,
        })
    }
}

impl Block for Hueshift {
    fn update(&mut self) -> Result<Option<Update>> {
        self.text.set_text(&self.current_temp.to_string());
        Ok(Some(self.update_interval.into()))
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.text]
    }

    fn click(&mut self, event: &I3BarEvent) -> Result<()> {
        if event.name.is_some() {
            match event.button {
                mb => {
                    use LogicalDirection::*;
                    match self.config.scrolling.to_logical_direction(mb) {
                        Some(Up) => {
                            let new_temp: u16 = self.current_temp + self.step;
                            if new_temp < self.max_temp {
                                update_hue(new_temp);
                                self.current_temp = new_temp;
                            }
                        }
                        Some(Down) => {
                            let new_temp: u16 = self.current_temp - self.step;
                            if new_temp > self.min_temp {
                                update_hue(new_temp);
                                self.current_temp = new_temp;
                            }
                        }
                        None => {}
                    }
                }
            }
        }
        Ok(())
    }

    fn id(&self) -> &str {
        &self.id
    }
}

/// Currently, detects whether sct and redshift are installed.
#[inline]
fn what_is_supported() -> (Result<bool>, Result<bool>) {
    (has_command("hueshift", "redshift"), has_command("hueshift", "sct"))
}
#[inline]
fn update_hue(new_temp: u16) {
    // TODO: Add code to use hue_shifter (if it is sct or redshift)
    Command::new("sh")
        .args(&["-c", format!("redshift -O {}", new_temp).as_str(), ">/dev/null 2>&1"])
        .spawn()
        .expect("Failed to set new color temperature using redshift.");
}

use std::process::Command;
use std::time::Duration;

use crossbeam_channel::Sender;
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::SharedConfig;
use crate::config::{LogicalDirection, Scrolling};
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::protocol::i3bar_event::{I3BarEvent, MouseButton};
use crate::scheduler::Task;
use crate::util::has_command;
use crate::widgets::text::TextWidget;
use crate::widgets::I3BarWidget;

pub struct Hueshift {
    id: usize,
    text: TextWidget,
    update_interval: Duration,
    step: u16,
    current_temp: u16,
    max_temp: u16,
    min_temp: u16,
    hue_shift_driver: Box<dyn HueShiftDriver>,
    click_temp: u16,
    scrolling: Scrolling,
}

trait HueShiftDriver {
    fn update(&self, temp: u16) -> Result<()>;
    fn reset(&self) -> Result<()>;
}
struct Redshift();
impl HueShiftDriver for Redshift {
    fn update(&self, temp: u16) -> Result<()> {
        Command::new("sh")
            .args(&[
                "-c",
                format!("redshift -O {} -P >/dev/null 2>&1", temp).as_str(),
            ])
            .spawn()
            .block_error(
                "hueshift",
                "Failed to set new color temperature using redshift.",
            )?;
        Ok(())
    }
    fn reset(&self) -> Result<()> {
        Command::new("sh")
            .args(&["-c", "redshift -x >/dev/null 2>&1"])
            .spawn()
            .block_error(
                "redshift",
                "Failed to set new color temperature using redshift.",
            )?;
        Ok(())
    }
}
struct Sct();
impl HueShiftDriver for Sct {
    fn update(&self, temp: u16) -> Result<()> {
        Command::new("sh")
            .args(&["-c", format!("sct {} >/dev/null 2>&1", temp).as_str()])
            .spawn()
            .block_error("hueshift", "Failed to set new color temperature using sct.")?;
        Ok(())
    }
    fn reset(&self) -> Result<()> {
        Command::new("sh")
            .args(&["-c", "sct >/dev/null 2>&1"])
            .spawn()
            .block_error("hueshift", "Failed to set new color temperature using sct.")?;
        Ok(())
    }
}
struct Gammastep();
impl HueShiftDriver for Gammastep {
    fn update(&self, temp: u16) -> Result<()> {
        Command::new("sh")
            .args(&[
                "-c",
                &format!("killall gammastep; gammastep -O {} -P &", temp),
            ])
            .spawn()
            .block_error(
                "hueshift",
                "Failed to set new color temperature using gammastep.",
            )?;
        Ok(())
    }
    fn reset(&self) -> Result<()> {
        Command::new("sh")
            .args(&["-c", "gammastep -x >/dev/null 2>&1"])
            .spawn()
            .block_error(
                "hueshift",
                "Failed to set new color temperature using gammastep.",
            )?;
        Ok(())
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "lowercase")]
pub enum HueShifter {
    Redshift,
    Sct,
    Gammastep,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct HueshiftConfig {
    /// Update interval in seconds
    #[serde(deserialize_with = "deserialize_duration")]
    pub interval: Duration,

    pub max_temp: u16,
    pub min_temp: u16,

    // TODO: Detect currently defined temperature
    /// Currently defined temperature default to 6500K.
    pub current_temp: u16,

    /// Can be set by user as an option.
    pub hue_shifter: Option<HueShifter>,

    /// Default to 100K, cannot go over 500K.
    pub step: u16,
    pub click_temp: u16,
}

impl Default for HueshiftConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(5),
            max_temp: 10_000,
            min_temp: 1_000,
            current_temp: 6_500,
            hue_shifter: if has_command("hueshift", "redshift").unwrap_or(false) {
                Some(HueShifter::Redshift)
            } else if has_command("hueshift", "sct").unwrap_or(false) {
                Some(HueShifter::Sct)
            } else if has_command("hueshift", "gammastep").unwrap_or(false) {
                Some(HueShifter::Gammastep)
            } else {
                None
            },
            step: 100,
            click_temp: 6_500,
        }
    }
}

impl ConfigBlock for Hueshift {
    type Config = HueshiftConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        shared_config: SharedConfig,
        _tx_update_request: Sender<Task>,
    ) -> Result<Self> {
        let current_temp = block_config.current_temp;
        let mut step = block_config.step;
        let mut max_temp = block_config.max_temp;
        let mut min_temp = block_config.min_temp;
        // limit too big steps at 500K to avoid too brutal changes
        if step > 500 {
            step = 500;
        }
        if block_config.max_temp > 10_000 {
            max_temp = 10_000;
        }
        if block_config.min_temp < 1000 || block_config.min_temp > block_config.max_temp {
            min_temp = 1000;
        }

        let hue_shift_driver: Box<dyn HueShiftDriver> = match block_config
            .hue_shifter
            .block_error("hueshift", "Cound not detect driver program")?
        {
            HueShifter::Redshift => Box::new(Redshift {}),
            HueShifter::Sct => Box::new(Sct {}),
            HueShifter::Gammastep => Box::new(Gammastep {}),
        };

        Ok(Hueshift {
            id,
            update_interval: block_config.interval,
            step,
            max_temp,
            min_temp,
            current_temp,
            hue_shift_driver,
            click_temp: block_config.click_temp,
            scrolling: shared_config.scrolling,
            text: TextWidget::new(id, 0, shared_config).with_text(&current_temp.to_string()),
        })
    }
}

impl Block for Hueshift {
    fn update(&mut self) -> Result<Option<Update>> {
        self.text.set_text(self.current_temp.to_string());
        Ok(Some(self.update_interval.into()))
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.text]
    }

    fn click(&mut self, event: &I3BarEvent) -> Result<()> {
        match event.button {
            MouseButton::Left => {
                self.current_temp = self.click_temp;
                self.hue_shift_driver.update(self.current_temp)?;
            }
            MouseButton::Right => {
                if self.max_temp > 6500 {
                    self.current_temp = 6500;
                    self.hue_shift_driver.reset()?;
                } else {
                    self.current_temp = self.max_temp;
                    self.hue_shift_driver.update(self.current_temp)?;
                }
            }
            mb => {
                use LogicalDirection::*;
                let new_temp: u16;
                match self.scrolling.to_logical_direction(mb) {
                    Some(Up) => {
                        new_temp = self.current_temp + self.step;
                        if new_temp <= self.max_temp {
                            self.hue_shift_driver.update(new_temp)?;
                            self.current_temp = new_temp;
                        }
                    }
                    Some(Down) => {
                        new_temp = self.current_temp - self.step;
                        if new_temp >= self.min_temp {
                            self.hue_shift_driver.update(new_temp)?;
                            self.current_temp = new_temp;
                        }
                    }
                    None => {}
                }
            }
        }
        Ok(())
    }

    fn id(&self) -> usize {
        self.id
    }
}

//! Manage display temperature
//!
//! This block displays the current color temperature in Kelvin. When scrolling upon the block the color temperature is changed.
//! A left click on the block sets the color temperature to `click_temp` that is by default to `6500K`.
//! A right click completely resets the color temperature to its default value (`6500K`).
//!
//! # Configuration
//!
//! Key | Values | Required | Default
//! ----|--------|----------|--------
//! `step`        | The step color temperature is in/decreased in Kelvin. | No | `100`
//! `hue_shifter` | Program used to control screen color. | No | Detect automatically. |
//! `max_temp`    | Max color temperature in Kelvin. | No | `10000`
//! `min_temp`    | Min color temperature in Kelvin. | No | `1000`
//! `click_temp`  | Left click color temperature in Kelvin. | No | `6500`
//!
//! # Available Hue Shifters
//!
//! Name | Supports
//! -----|---------
//! `"redshift"`  | X11
//! `"sct"`       | X11
//! `"gammastep"` | X11 and Wayland
//!
//! # Example
//!
//! ```toml
//! [[block]]
//! block = "hueshift"
//! hue_shifter = "redshift"
//! step = 50
//! click_temp = 3500
//! ```
//!
//! A hard limit is set for the `max_temp` to `10000K` and the same for the `min_temp` which is `1000K`.
//! The `step` has a hard limit as well, defined to `500K` to avoid too brutal changes.

use super::prelude::*;
use crate::util::has_command;
use std::process::Command;

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields, default)]
struct HueshiftConfig {
    interval: Seconds,
    max_temp: u16,
    min_temp: u16,
    // TODO: Detect currently defined temperature
    current_temp: u16,
    hue_shifter: Option<HueShifter>,
    step: u16,
    click_temp: u16,
}

impl Default for HueshiftConfig {
    fn default() -> Self {
        Self {
            interval: Seconds::new(5),
            max_temp: 10_000,
            min_temp: 1_000,
            current_temp: 6_500,
            hue_shifter: None,
            step: 100,
            click_temp: 6_500,
        }
    }
}

pub async fn run(config: toml::Value, mut api: CommonApi) -> Result<()> {
    let mut events = api.get_events().await?;
    let config = HueshiftConfig::deserialize(config).config_error()?;

    // limit too big steps at 500K to avoid too brutal changes
    let step = config.step.max(500);
    let max_temp = config.max_temp.min(10_000);
    let min_temp = config.min_temp.clamp(1_000, max_temp);

    let hue_shifter = match config.hue_shifter {
        Some(driver) => driver,
        None => {
            if has_command("redshift").await? {
                HueShifter::Redshift
            } else if has_command("sct").await? {
                HueShifter::Sct
            } else if has_command("gammastep").await? {
                HueShifter::Gammastep
            } else if has_command("wlsunset").await? {
                HueShifter::Wlsunset
            } else {
                return Err(Error::new("Cound not detect driver program"));
            }
        }
    };

    let driver: Box<dyn HueShiftDriver> = match hue_shifter {
        HueShifter::Redshift => Box::new(Redshift),
        HueShifter::Sct => Box::new(Sct),
        HueShifter::Gammastep => Box::new(Gammastep),
        HueShifter::Wlsunset => Box::new(Wlsunset),
    };

    let mut current_temp = config.current_temp;

    loop {
        api.set_text(current_temp.to_string().into());
        api.flush().await?;

        tokio::select! {
            _ = sleep(config.interval.0) => (),
            Some(BlockEvent::Click(click)) = events.recv() => {
                match click.button {
                    MouseButton::Left => {
                        current_temp = config.click_temp;
                        driver.update(current_temp)?;
                    }
                    MouseButton::Right => {
                        if max_temp > 6500 {
                            current_temp = 6500;
                            driver.reset()?;
                        } else {
                            current_temp = max_temp;
                            driver.update(current_temp)?;
                        }
                    }
                    MouseButton::WheelUp => {
                        current_temp = (current_temp + step).min(max_temp);
                        driver.update(current_temp)?;
                    }
                    MouseButton::WheelDown => {
                        current_temp = current_temp.saturating_sub(step).max(min_temp);
                        driver.update(current_temp)?;
                    }
                    _ => (),
                }
            }
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "lowercase")]
enum HueShifter {
    Redshift,
    Sct,
    Gammastep,
    Wlsunset,
}

trait HueShiftDriver {
    fn update(&self, temp: u16) -> Result<()>;
    fn reset(&self) -> Result<()>;
}

struct Redshift;
impl HueShiftDriver for Redshift {
    fn update(&self, temp: u16) -> Result<()> {
        Command::new("sh")
            .args(&[
                "-c",
                format!("redshift -O {} -P >/dev/null 2>&1", temp).as_str(),
            ])
            .spawn()
            .error("Failed to set new color temperature using redshift.")?;
        Ok(())
    }
    fn reset(&self) -> Result<()> {
        Command::new("sh")
            .args(&["-c", "redshift -x >/dev/null 2>&1"])
            .spawn()
            .error("Failed to set new color temperature using redshift.")?;
        Ok(())
    }
}

struct Sct;
impl HueShiftDriver for Sct {
    fn update(&self, temp: u16) -> Result<()> {
        Command::new("sh")
            .args(&["-c", format!("sct {} >/dev/null 2>&1", temp).as_str()])
            .spawn()
            .error("Failed to set new color temperature using sct.")?;
        Ok(())
    }
    fn reset(&self) -> Result<()> {
        Command::new("sh")
            .args(&["-c", "sct >/dev/null 2>&1"])
            .spawn()
            .error("Failed to set new color temperature using sct.")?;
        Ok(())
    }
}

struct Gammastep;
impl HueShiftDriver for Gammastep {
    fn update(&self, temp: u16) -> Result<()> {
        Command::new("sh")
            .args(&[
                "-c",
                &format!("killall gammastep; gammastep -O {} -P &", temp),
            ])
            .spawn()
            .error("Failed to set new color temperature using gammastep.")?;
        Ok(())
    }
    fn reset(&self) -> Result<()> {
        Command::new("sh")
            .args(&["-c", "gammastep -x >/dev/null 2>&1"])
            .spawn()
            .error("Failed to set new color temperature using gammastep.")?;
        Ok(())
    }
}

struct Wlsunset;
impl HueShiftDriver for Wlsunset {
    fn update(&self, temp: u16) -> Result<()> {
        Command::new("sh")
            // wlsunset does not have a oneshot option, so set both day and
            // night temperature. wlsunset dose not allow for day and night
            // temperatures to be the same, so increment the day temperature.
            .args(&[
                "-c",
                &format!("killall wlsunset; wlsunset -T {} -t {} &", temp + 1, temp),
            ])
            .spawn()
            .error("Failed to set new color temperature using wlsunset.")?;
        Ok(())
    }
    fn reset(&self) -> Result<()> {
        Command::new("sh")
            // wlsunset does not have a reset option, so just kill the process.
            // Trying to call wlsunset without any arguments uses the defaults:
            // day temp: 6500K
            // night temp: 4000K
            // latitude/longitude: NaN
            //     ^ results in sun_condition == POLAR_NIGHT at time of testing
            // With these defaults, this results in the the color temperature
            // getting set to 4000K.
            .args(&["-c", "killall wlsunset > /dev/null 2>&1"])
            .spawn()
            .error("Failed to set new color temperature using wlsunset.")?;
        Ok(())
    }
}

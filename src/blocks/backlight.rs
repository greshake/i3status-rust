//! The brightness of a backlight device
//!
//! This block reads brightness information directly from the filesystem, so it works under both
//! X11 and Wayland. The block uses `inotify` to listen for changes in the device's brightness
//! directly, so there is no need to set an update interval. This block uses DBus to set brightness
//! level using the mouse wheel, but will [fallback to sysfs](#d-bus-fallback) if `systemd-logind` is not used.
//!
//! # Root scaling
//!
//! Some devices expose raw values that are best handled with nonlinear scaling. The human perception of lightness is close to the cube root of relative luminance, so settings for `root_scaling` between 2.4 and 3.0 are worth trying. For devices with few discrete steps this should be 1.0 (linear). More information: <https://en.wikipedia.org/wiki/Lightness>
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `device` | A regex to match against `/sys/class/backlight` devices to read brightness information from (can match 1 or more devices). When there is no `device` specified, this block will display information for all devices found in the `/sys/class/backlight` directory. | Default device
//! `format` | A string to customise the output of this block. See below for available placeholders. | `" $icon $brightness "`
//! `missing_format` | A string to customise the output of this block. No placeholders available | `" no backlight devices "`
//! `step_width` | The brightness increment to use when scrolling, in percent | `5`
//! `minimum` | The minimum brightness that can be scrolled down to | `5`
//! `maximum` | The maximum brightness that can be scrolled up to | `100`
//! `cycle` | The brightnesses to cycle through on each click | `[minimum, maximum]`
//! `root_scaling` | Scaling exponent reciprocal (ie. root) | `1.0`
//! `invert_icons` | Invert icons' ordering, useful if you have colorful emoji | `false`
//! `ddcci_sleep_multiplier` | [See ddcutil documentation](https://www.ddcutil.com/performance_options/#option-sleep-multiplier) | `1.0`
//! `ddcci_max_tries_write_read` | The maximum number of times to attempt writing to  or reading from a ddcci monitor | `10`
//!
//! Placeholder  | Value                                     | Type   | Unit
//! -------------|-------------------------------------------|--------|---------------
//! `icon`       | Icon based on backlight's state           | Icon   | -
//! `brightness` | Current brightness                        | Number | %
//!
//! Action            | Default button
//! ------------------|---------------
//! `cycle`           | Left
//! `brightness_up`   | Wheel Up
//! `brightness_down` | Wheel Down
//!
//! # Example
//!
//! ```toml
//! [[block]]
//! block = "backlight"
//! device = "intel_backlight"
//! ```
//!
//! Hide missing backlight:
//!
//! ```toml
//! [[block]]
//! block = "backlight"
//! missing_format = ""
//! ```
//!
//! # calibright
//!
//! Additional display brightness calibration can be set in `$XDG_CONFIG_HOME/calibright/config.toml`
//! See <https://github.com/bim9262/calibright> for more details.
//! This block will override any global config set in `$XDG_CONFIG_HOME/calibright/config.toml`
//!
//! # D-Bus Fallback
//!
//! If you don't use `systemd-logind` i3status-rust will attempt to set the brightness
//! using sysfs. In order to do this you'll need to have write permission.
//! You can do this by writing a `udev` rule for your system.
//!
//! First, check that your user is a member of the "video" group using the
//! `groups` command. Then add a rule in the `/etc/udev/rules.d/` directory
//! containing the following, for example in `backlight.rules`:
//!
//! ```text
//! ACTION=="add", SUBSYSTEM=="backlight", GROUP="video", MODE="0664"
//! ```
//!
//! This will allow the video group to modify all backlight devices. You will
//! also need to restart for this rule to take effect.
//!
//! # Icons Used
//! - `backlight` (as a progression)

use std::sync::Arc;

use calibright::{CalibrightBuilder, CalibrightConfig, CalibrightError, DeviceConfig};

use super::prelude::*;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub device: Option<String>,
    pub format: FormatConfig,
    pub missing_format: FormatConfig,
    #[default(5.0)]
    pub step_width: f64,
    #[default(5.0)]
    pub minimum: f64,
    #[default(100.0)]
    pub maximum: f64,
    pub cycle: Option<Vec<f64>>,
    pub invert_icons: bool,
    //Calibright config settings
    pub root_scaling: Option<f64>,
    pub ddcci_sleep_multiplier: Option<f64>,
    pub ddcci_max_tries_write_read: Option<u8>,
}

pub async fn run(config: &Config, api: &CommonApi) -> Result<()> {
    let mut actions = api.get_actions()?;
    api.set_default_actions(&[
        (MouseButton::Left, None, "cycle"),
        (MouseButton::WheelUp, None, "brightness_up"),
        (MouseButton::WheelDown, None, "brightness_down"),
    ])?;

    let format = config.format.with_default(" $icon $brightness ")?;
    let missing_format = config
        .missing_format
        .with_default(" no backlight devices ")?;

    let default_cycle = &[config.minimum, config.maximum];
    let mut cycle = config
        .cycle
        .as_deref()
        .unwrap_or(default_cycle)
        .iter()
        .map(|x| x / 100.0)
        .cycle();

    let step_width = config.step_width / 100.0;
    let minimum = config.minimum / 100.0;
    let maximum = config.maximum / 100.0;

    let mut calibright_defaults = DeviceConfig::default();

    if let Some(root_scaling) = config.root_scaling {
        calibright_defaults.root_scaling = root_scaling;
    }

    if let Some(ddcci_sleep_multiplier) = config.ddcci_sleep_multiplier {
        calibright_defaults.ddcci_sleep_multiplier = ddcci_sleep_multiplier;
    }

    if let Some(ddcci_max_tries_write_read) = config.ddcci_max_tries_write_read {
        calibright_defaults.ddcci_max_tries_write_read = ddcci_max_tries_write_read;
    }

    let calibright_config = CalibrightConfig::new_with_defaults(&calibright_defaults)
        .await
        .error("calibright config error")?;

    let mut calibright = CalibrightBuilder::new()
        .with_device_regex(config.device.as_deref().unwrap_or("."))
        .with_config(calibright_config)
        .with_poll_interval(api.error_interval)
        .build()
        .await
        .error("Failed to init calibright")?;

    // This is used to display the error, if there is one
    let mut block_error: Option<CalibrightError> = None;

    let mut brightness = calibright
        .get_brightness()
        .await
        .map_err(|e| block_error = Some(e))
        .unwrap_or_default();

    loop {
        match block_error {
            Some(CalibrightError::NoDevices) => {
                let widget = Widget::new()
                    .with_format(missing_format.clone())
                    .with_state(State::Critical);
                api.set_widget(widget)?;
            }
            Some(e) => {
                api.set_error(Error {
                    message: None,
                    cause: Some(Arc::new(e)),
                })?;
            }
            None => {
                let mut widget = Widget::new().with_format(format.clone());
                let mut icon_value = brightness;
                if config.invert_icons {
                    icon_value = 1.0 - icon_value;
                }
                widget.set_values(map! {
                    "icon" => Value::icon_progression("backlight", icon_value),
                    "brightness" => Value::percents((brightness * 100.0).round())
                });
                api.set_widget(widget)?;
            }
        }

        loop {
            select! {
                // Calibright can recover from errors, just keep reading the next event.
                _ = calibright.next() => {
                    block_error = calibright
                        .get_brightness()
                        .await
                        .map(|new_brightness| {brightness = new_brightness;})
                        .err();

                    break;
                },
                Some(action) = actions.recv() => match action.as_ref() {
                    "cycle" => {
                        if let Some(cycle_brightness) = cycle.next() {
                            brightness = cycle_brightness;
                            block_error = calibright
                                .set_brightness(brightness)
                                .await
                                .err();
                            break;

                        }
                    }
                    "brightness_up" => {
                        brightness = (brightness + step_width).clamp(minimum, maximum);
                        block_error = calibright
                            .set_brightness(brightness)
                            .await
                            .err();
                        break;
                    }
                    "brightness_down" => {
                        brightness = (brightness - step_width).clamp(minimum, maximum);
                        block_error = calibright
                            .set_brightness(brightness)
                            .await
                            .err();
                        break;
                    }
                    _ => (),
                }
            }
        }
    }
}

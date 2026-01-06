//! X11 screen information
//!
//! X11 screen information (name, brightness, resolution, refresh rate). With a click you can toggle through your active screens, and scrolling the wheel up/down, you can adjust the selected screens' brightness. Regarding brightness control, xrandr changes the brightness of the display using gamma rather than changing the brightness in hardware, so if that is not desirable, then consider using the `backlight` block instead.
//!
//! NOTE: Some users report issues (e.g. [here](https://github.com/greshake/i3status-rust/issues/274) and [here](https://github.com/greshake/i3status-rust/issues/668) when using this block. The cause is currently unknown, however, setting a higher update interval may help.
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `format` | A string to customise the output of this block. See below for available placeholders. | `" $icon $display $brightness_icon $brightness "`
//! `step_width` | The steps brightness is in/decreased for the selected screen (When greater than 50 it gets limited to 50). | `5`
//! `interval` | Update interval in seconds. | `5`
//! `invert_icons` | Invert icons' ordering, useful if you have colourful emoji | `false`
//!
//! Placeholder       | Value                        | Type   | Unit
//! ------------------|------------------------------|--------|---------------
//! `icon`            | A static icon                | Icon   | -
//! `display`         | The monitor name             | Text   | -
//! `brightness`      | The monitor brightness       | Number | %
//! `brightness_icon` | A static icon                | Icon   | -
//! `resolution`      | The monitor resolution       | Text   | -
//! `res_icon`        | A static icon                | Icon   | -
//! `refresh_rate`    | The monitor refresh rate     | Number | Hertz
//!
//! Action            | Default button
//! ------------------|---------------
//! `cycle_outputs`   | Left
//! `brightness_up`   | Wheel Up
//! `brightness_down` | Wheel Down
//!
//! # Example
//!
//! ```toml
//! [[block]]
//! block = "xrandr"
//! format = " $icon $brightness $resolution $refresh_rate "
//! ```
//!
//! # Used Icons
//! - `xrandr`
//! - `backlight`
//! - `resolution`

use super::prelude::*;
use crate::subprocess::spawn_shell;
use tokio::process::Command;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    #[default(5.into())]
    pub interval: Seconds,
    pub format: FormatConfig,
    #[default(5)]
    pub step_width: u32,
    pub invert_icons: bool,
}

pub async fn run(config: &Config, api: &CommonApi) -> Result<()> {
    let mut actions = api.get_actions()?;
    api.set_default_actions(&[
        (MouseButton::Left, None, "cycle_outputs"),
        (MouseButton::WheelUp, None, "brightness_up"),
        (MouseButton::WheelDown, None, "brightness_down"),
    ])?;

    let format = config
        .format
        .with_default(" $icon $display $brightness_icon $brightness ")?;

    let mut cur_index = 0;
    let mut timer = config.interval.timer();

    loop {
        let mut monitors = get_monitors().await?;
        if cur_index > monitors.len() {
            cur_index = 0;
        }

        loop {
            let mut widget = Widget::new().with_format(format.clone());

            if let Some(mon) = monitors.get(cur_index) {
                let mut icon_value = mon.brightness as f64;
                if config.invert_icons {
                    icon_value = 1.0 - icon_value;
                }
                widget.set_values(map! {
                    "icon" => Value::icon("xrandr"),
                    "display" => Value::text(mon.name.clone()),
                    "brightness" => Value::percents(mon.brightness_percent()),
                    "brightness_icon" => Value::icon_progression("backlight", icon_value),
                    "resolution" => Value::text(mon.resolution()),
                    "res_icon" => Value::icon("resolution"),
                    "refresh_rate" => Value::hertz(mon.refresh_hz),
                });
            }
            api.set_widget(widget)?;

            select! {
                _ = timer.tick() => break,
                _ = api.wait_for_update_request() => break,
                Some(action) = actions.recv() => match action.as_ref() {
                    "cycle_outputs" => {
                        cur_index = (cur_index + 1) % monitors.len();
                    }
                    "brightness_up" => {
                        if let Some(monitor) = monitors.get_mut(cur_index) {
                            let bright = (monitor.brightness_percent() + config.step_width).min(100);
                            monitor.set_brightness_percent(bright)?;
                        }
                    }
                    "brightness_down" => {
                        if let Some(monitor) = monitors.get_mut(cur_index) {
                            let bright = monitor.brightness_percent().saturating_sub(config.step_width);
                            monitor.set_brightness_percent(bright)?;
                        }
                    }
                    _ => (),
                }
            }
        }
    }
}

#[derive(Debug, PartialEq)]
struct Monitor {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
    pub brightness: f32,
    pub refresh_hz: f64,
}

impl Monitor {
    fn set_brightness_percent(&mut self, percent: u32) -> Result<()> {
        let brightness = percent as f32 / 100.0;
        spawn_shell(&format!(
            "xrandr --output {} --brightness {}",
            self.name, brightness
        ))
        .error(format!(
            "Failed to set brightness {} for output {}",
            brightness, self.name
        ))?;
        self.brightness = brightness;
        Ok(())
    }

    #[inline]
    fn resolution(&self) -> String {
        format!("{}x{}", self.width, self.height)
    }

    #[inline]
    fn brightness_percent(&self) -> u32 {
        (self.brightness * 100.0) as u32
    }
}

async fn get_monitors() -> Result<Vec<Monitor>> {
    let monitors_info = Command::new("xrandr")
        .arg("--verbose")
        .output()
        .await
        .error("Failed to collect xrandr monitors info")?
        .stdout;
    let monitors_info =
        String::from_utf8(monitors_info).error("xrandr produced non-UTF8 output")?;

    Ok(parser::extract_outputs(&monitors_info))
}

mod parser {
    use super::*;
    use nom::IResult;
    use nom::branch::alt;
    use nom::bytes::complete::{tag, take_until, take_while1};
    use nom::character::complete::{i32, space0, space1, u32};
    use nom::combinator::opt;
    use nom::number::complete::{double, float};
    use nom::sequence::preceded;

    /// Parses an output name, e.g. "HDMI-0", "eDP-1", etc.
    fn name(input: &str) -> IResult<&str, &str> {
        take_while1(|c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')(input)
    }

    /// Parses "1920x1080+0+0"
    /// Returns (width, height, x, y)
    fn parse_mode_position(input: &str) -> IResult<&str, (u32, u32, i32, i32)> {
        let (input, width) = u32(input)?;
        let (input, _) = tag("x")(input)?;
        let (input, height) = u32(input)?;
        let (input, _) = tag("+")(input)?;
        let (input, x) = i32(input)?;
        let (input, _) = tag("+")(input)?;
        let (input, y) = i32(input)?;
        Ok((input, (width, height, x, y)))
    }

    /// Parses "HDMI-1 connected primary 2560x1440+1920+0 ..."
    /// Returns (name, width, height, x, y)
    fn parse_output_header(input: &str) -> IResult<&str, (String, u32, u32, i32, i32)> {
        let (input, name) = name(input)?;
        let (input, _) = space1(input)?;
        let (input, _) = alt((tag("connected"), tag("disconnected")))(input)?;
        let (input, _) = opt(preceded(space1, tag("primary")))(input)?;
        let (input, _) = space1(input)?;
        let (input, (width, height, x, y)) = parse_mode_position(input)?;
        Ok((input, (name.to_owned(), width, height, x, y)))
    }

    /// Parses "    Brightness: 1.0"
    fn parse_brightness(input: &str) -> IResult<&str, f32> {
        let (input, _) = space0(input)?;
        let (input, _) = tag("Brightness: ")(input)?;
        let (input, brightness) = float(input)?;
        Ok((input, brightness))
    }

    /// Parses "    v: ... clock  74.97Hz"
    fn parse_v_clock_hz(input: &str) -> IResult<&str, f64> {
        let (input, _) = space0(input)?;
        let (input, _) = tag("v:")(input)?;
        let (input, _) = take_until("clock")(input)?;
        let (input, _) = tag("clock")(input)?;
        let (input, _) = space1(input)?;
        let (input, hz) = double(input)?;
        let (input, _) = tag("Hz")(input)?;
        Ok((input, hz))
    }

    /// Returns `true` if this is the starting line for the current mode.
    ///
    /// Examples:
    /// - "  2560x1440 (0x1d6) ... *current"
    /// - "  2560x1440 (0x4b)  144.00*+ 120.00 ..."
    #[inline]
    fn is_current_mode(line: &str) -> bool {
        line.starts_with("  ")
            && (line.contains("*current") || (line.contains("(0x") && line.contains("*")))
    }

    /// Parse the outputs from `xrandr --verbose` output.
    pub fn extract_outputs(input: &str) -> Vec<Monitor> {
        let mut outputs = Vec::new();

        let lines = input.lines().collect::<Vec<_>>();
        let mut i = 0;
        while i < lines.len() {
            // Find header
            let Ok((_, (name, width, height, x, y))) = parse_output_header(lines[i]) else {
                i += 1;
                continue;
            };

            // Scan for brightness/refresh_hz until the next header
            let mut brightness = None;
            let mut refresh_hz = None;

            i += 1;
            while i < lines.len() {
                if parse_output_header(lines[i]).is_ok() {
                    // found the next header
                    break;
                }

                if brightness.is_none() {
                    brightness = parse_brightness(lines[i]).ok().map(|(_, b)| b);
                }

                if refresh_hz.is_none() && is_current_mode(lines[i]) {
                    // find the next v-clock line
                    i += 1;
                    while i < lines.len() {
                        if parse_output_header(lines[i]).is_ok() {
                            // found the next header
                            i -= 1;
                            break;
                        }

                        if let Ok((_, hz)) = parse_v_clock_hz(lines[i]) {
                            refresh_hz = Some(hz);
                            break;
                        }

                        i += 1;
                    }
                }

                i += 1;
            }

            outputs.push(Monitor {
                name,
                width,
                height,
                x,
                y,
                brightness: brightness.unwrap_or_default(),
                refresh_hz: refresh_hz.unwrap_or_default(),
            });
        }

        outputs
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_extract_outputs() {
            let xrandr_output = include_str!("../../testdata/xrandr-verbose.txt");
            let outputs = extract_outputs(xrandr_output);
            assert_eq!(outputs.len(), 2);
            assert_eq!(
                outputs[0],
                Monitor {
                    name: "eDP-1".to_owned(),
                    width: 1920,
                    height: 1080,
                    x: 0,
                    y: 1080,
                    brightness: 1.0,
                    refresh_hz: 59.96,
                }
            );
            assert_eq!(
                outputs[1],
                Monitor {
                    name: "HDMI-1".to_owned(),
                    width: 1920,
                    height: 1080,
                    x: 0,
                    y: 0,
                    brightness: 0.8,
                    refresh_hz: 59.99,
                }
            );
        }
    }
}

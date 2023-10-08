//! X11 screen information
//!
//! X11 screen information (name, brightness, resolution). With a click you can toggle through your active screens and with wheel up and down you can adjust the selected screens brightness. Regarding brightness control, xrandr changes the brightness of the display using gamma rather than changing the brightness in hardware, so if that is not desirable then consider using the `backlight` block instead.
//!
//! NOTE: Some users report issues (e.g. [here](https://github.com/greshake/i3status-rust/issues/274) and [here](https://github.com/greshake/i3status-rust/issues/668) when using this block. The cause is currently unknown, however setting a higher update interval may help.
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `format` | A string to customise the output of this block. See below for available placeholders. | `" $icon $display $brightness_icon $brightness "`
//! `step_width` | The steps brightness is in/decreased for the selected screen (When greater than 50 it gets limited to 50). | `5`
//! `interval` | Update interval in seconds. | `5`
//!
//! Placeholder       | Value                        | Type   | Unit
//! ------------------|------------------------------|--------|---------------
//! `icon`            | A static icon                | Icon   | -
//! `display`         | The name of a monitor        | Text   | -
//! `brightness`      | The brightness of a monitor  | Number | %
//! `brightness_icon` | A static icon                | Icon   | -
//! `resolution`      | The resolution of a monitor  | Text   | -
//! `res_icon`        | A static icon                | Icon   | -
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
//! format = " $icon $brightness $resolution "
//! ```
//!
//! # Used Icons
//! - `xrandr`
//! - `backlight`
//! - `resolution`

use super::prelude::*;
use crate::subprocess::spawn_shell;
use regex::RegexSet;
use tokio::process::Command;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    #[default(5.into())]
    pub interval: Seconds,
    pub format: FormatConfig,
    #[default(5)]
    pub step_width: u32,
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

    let mut cur_indx = 0;
    let mut timer = config.interval.timer();

    loop {
        let mut monitors = get_monitors().await?;
        if cur_indx > monitors.len() {
            cur_indx = 0;
        }

        loop {
            let mut widget = Widget::new().with_format(format.clone());

            if let Some(mon) = monitors.get(cur_indx) {
                widget.set_values(map! {
                    "display" => Value::text(mon.name.clone()),
                    "brightness" => Value::percents(mon.brightness),
                    //TODO: change `brightness_icon` based on `brightness`
                    "brightness_icon" => Value::icon("backlight"),
                    "resolution" => Value::text(mon.resolution.clone()),
                    "icon" => Value::icon("xrandr"),
                    "res_icon" => Value::icon("resolution"),
                });
            }
            api.set_widget(widget)?;

            select! {
                _ = timer.tick() => break,
                _ = api.wait_for_update_request() => break,
                Some(action) = actions.recv() => match action.as_ref() {
                    "cycle_outputs" => {
                        cur_indx = (cur_indx + 1) % monitors.len();
                    }
                    "brightness_up" => {
                        if let Some(monitor) = monitors.get_mut(cur_indx) {
                            let bright = (monitor.brightness + config.step_width).min(100);
                            monitor.set_brightness(bright);
                        }
                    }
                    "brightness_down" => {
                        if let Some(monitor) = monitors.get_mut(cur_indx) {
                            let bright = monitor.brightness.saturating_sub(config.step_width);
                            monitor.set_brightness(bright);
                        }
                    }
                    _ => (),
                }
            }
        }
    }
}

struct Monitor {
    name: String,
    brightness: u32,
    resolution: String,
}

impl Monitor {
    fn set_brightness(&mut self, brightness: u32) {
        let _ = spawn_shell(&format!(
            "xrandr --output {} --brightness  {}",
            self.name,
            brightness as f64 / 100.0
        ));
        self.brightness = brightness;
    }
}

async fn get_monitors() -> Result<Vec<Monitor>> {
    let mut monitors = Vec::new();

    let active_monitors = Command::new("xrandr")
        .arg("--listactivemonitors")
        .output()
        .await
        .error("Failed to collect active xrandr monitors")?
        .stdout;
    let active_monitors =
        String::from_utf8(active_monitors).error("xrandr produced non-UTF8 output")?;

    let regex = active_monitors
        .lines()
        .filter_map(|line| line.split_ascii_whitespace().last())
        .map(|name| format!("{name} connected"))
        .chain(Some("Brightness:".into()));
    let regex = RegexSet::new(regex).error("Failed to create RegexSet")?;

    let monitors_info = Command::new("xrandr")
        .arg("--verbose")
        .output()
        .await
        .error("Failed to collect xrandr monitors info")?
        .stdout;
    let monitors_info =
        String::from_utf8(monitors_info).error("xrandr produced non-UTF8 output")?;

    let mut it = monitors_info.lines().filter(|line| regex.is_match(line));

    while let (Some(line1), Some(line2)) = (it.next(), it.next()) {
        let mut tokens = line1.split_ascii_whitespace().peekable();
        let name = tokens.next().error("Failed to parse xrandr output")?.into();
        let _ = tokens.next();

        // The output may be "<name> connected <resolution>" or "<name> connected primary <resolution>"
        let _ = tokens.next_if_eq(&"primary");

        let resolution = tokens
            .next()
            .and_then(|x| x.split('+').next())
            .error("Failed to parse xrandr output")?
            .into();

        let brightness = (line2
            .split(':')
            .nth(1)
            .error("Failed to parse xrandr output")?
            .trim()
            .parse::<f64>()
            .error("Failed to parse xrandr output")?
            * 100.0) as u32;

        monitors.push(Monitor {
            name,
            brightness,
            resolution,
        });
    }

    Ok(monitors)
}

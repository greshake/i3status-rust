//! X11 screen information
//!
//! X11 screen information (name, brightness, resolution). With a click you can toggle through your active screens and with wheel up and down you can adjust the selected screens brightness. Regarding brightness control, xrandr changes the brightness of the display using gamma rather than changing the brightness in hardware, so if that is not desirable then consider using the `backlight` block instead.
//!
//! NOTE: Some users report issues (e.g. [here](https://github.com/greshake/i3status-rust/issues/274) and [here](https://github.com/greshake/i3status-rust/issues/668) when using this block. The cause is currently unknown, however setting a higher update interval may help.
//!
//! # Configuration
//!
//! Key | Values | Required | Default
//! ----|--------|----------|--------
//! `format` | A string to customise the output of this block. See below for available placeholders. | No | `"$display $brightness_icon $brightness"`
//! `step_width` | The steps brightness is in/decreased for the selected screen (When greater than 50 it gets limited to 50). | No | `5`
//! `interval` | Update interval in seconds. | No | `5`
//!
//! Placeholder       | Value                        | Type   | Unit
//! ------------------|------------------------------|--------|---------------
//! `display`         | The name of a monitor        | Text   | -
//! `brightness`      | The brightness of a monitor  | Number | %
//! `brightness_icon` | A static icon                | Icon   | -
//! `resolution`      | The resolution of a monitor  | Text   | -
//! `res_icon`        | A static icon                | Icon   | -
//!
//! # Example
//!
//! ```toml
//! [[block]]
//! block = "xrandr"
//! format = "$brightness $resolution"
//! ```
//!
//! # Used Icons
//! - `xrandr`
//! - `backlight_full`
//! - `resolution`

use std::collections::HashMap;
use tokio::process::Command;

use super::prelude::*;
use crate::subprocess::spawn_shell;

use regex::RegexSet;

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields, default)]
struct XrandrConfig {
    interval: Seconds,
    format: FormatConfig,
    step_width: u32,
}

impl Default for XrandrConfig {
    fn default() -> Self {
        Self {
            interval: Seconds::new(5),
            format: Default::default(),
            step_width: 5,
        }
    }
}

pub async fn run(config: toml::Value, mut api: CommonApi) -> Result<()> {
    let mut events = api.get_events().await?;
    let config = XrandrConfig::deserialize(config).config_error()?;

    api.set_format(
        config
            .format
            .with_default("$display $brightness_icon $brightness")?,
    );
    api.set_icon("xrandr")?;

    let display = |monitor: Option<&Monitor>, api: &mut CommonApi| {
        if let Some(mon) = monitor {
            api.set_values(map! {
                "display" => Value::text(mon.name.clone()),
                "brightness" => Value::percents(mon.brightness),
                //TODO: change `brightness_icon` based on `brightness`
                "brightness_icon" => Value::Icon(api.get_icon("backlight_full")?),
                "resolution" => Value::text(mon.resolution.clone()),
                "res_icon" => Value::Icon(api.get_icon("resolution")?),
            });
        } else {
            api.set_values(HashMap::new());
        }
        Ok(())
    };

    let mut cur_indx = 0;
    let mut timer = config.interval.timer();

    loop {
        let mut monitors = get_monitors().await?;
        if cur_indx > monitors.len() {
            cur_indx = 0;
        }

        display(monitors.get(cur_indx), &mut api)?;
        api.flush().await?;

        loop {
            tokio::select! {
                _ = timer.tick() => break,
                Some(BlockEvent::Click(click)) = events.recv() => {
                    match click.button {
                        MouseButton::Left => {
                            cur_indx += 1;
                            if cur_indx >= monitors.len() {
                                cur_indx = 0;
                            }
                        }
                        MouseButton::WheelUp => {
                            if let Some(monitor) = monitors.get_mut(cur_indx) {
                                let bright = (monitor.brightness + config.step_width).min(100);
                                monitor.set_brightness(bright);
                            }
                        }
                        MouseButton::WheelDown => {
                            if let Some(monitor) = monitors.get_mut(cur_indx) {
                                let bright = monitor.brightness.saturating_sub(config.step_width);
                                monitor.set_brightness(bright);
                            }
                        }
                        _ => (),
                    }
                    display(monitors.get(cur_indx), &mut api)?;
                    api.flush().await?;
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

macro_rules! unwrap_or_break {
    ($e: expr) => {
        match $e {
            Some(e) => e,
            None => break,
        }
    };
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
        StdString::from_utf8(active_monitors).error("xrandr produced non-UTF8 output")?;

    let regex = active_monitors
        .lines()
        .filter_map(|line| line.split_ascii_whitespace().last())
        .map(|name| format!("{} connected", name))
        .chain(Some("Brightness:".into()));
    let regex = RegexSet::new(regex).error("Failed to create RegexSet")?;

    let monitors_info = Command::new("xrandr")
        .arg("--verbose")
        .output()
        .await
        .error("Failed to collect xrandr monitors info")?
        .stdout;
    let monitors_info =
        StdString::from_utf8(monitors_info).error("xrandr produced non-UTF8 output")?;

    let mut it = monitors_info.lines().filter(|line| regex.is_match(line));

    #[allow(clippy::while_let_loop)]
    loop {
        let line1 = unwrap_or_break!(it.next());
        let line2 = unwrap_or_break!(it.next());

        let mut tokens = line1.split_ascii_whitespace();
        let name = tokens.next().error("Failed to parse xrandr output")?.into();
        let _ = tokens.next();
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

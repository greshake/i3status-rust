use std::time::Duration;
use std::process::Command;
use std::str::FromStr;

use util::FormatTemplate;

use block::Block;
use widgets::button::ButtonWidget;
use widget::I3BarWidget;
use input::{I3BarEvent, MouseButton};

use serde_json::Value;
use uuid::Uuid;

struct Monitor {
    name: String,
    brightness: u32,
    resolution: String,
}

impl Monitor {
    fn new(name: &str, brightness: u32, resolution: &str) -> Self {
        Monitor {
            name: String::from(name),
            brightness: brightness,
            resolution: String::from(resolution),
        }
    }

    fn set_brightness(&mut self, step: i32) {
        Command::new("sh")
            .args(&["-c", format!("xrandr --output {} --brightness {}",
                                  self.name,
                                  (self.brightness as i32 + step) as f32 / 100.0).as_str()])
            .spawn().expect("Failed to set xrandr output.");
        self.brightness = (self.brightness as i32 + step) as u32;
    }
}

pub struct Xrandr {
    text: ButtonWidget,
    id: String,
    update_interval: Duration,
    monitors: Vec<Monitor>,
    icons: bool,
    resolution: bool,
    step_width: u32,
    current_idx: usize,

    #[allow(dead_code)]
    theme: Value,
}

macro_rules! unwrap_or_continue {
    ($e: expr) => (
        match $e {
            Some(e) => e,
            None => continue,
        }
    )
}

impl Xrandr {
    pub fn new(config: Value, theme: Value) -> Xrandr {
        {
            let id = Uuid::new_v4().simple().to_string();
            let mut step_width = get_u64_default!(config, "step_width", 5) as u32;
            if step_width > 50 {
                step_width = 50;
            }
            Xrandr {
                text: ButtonWidget::new(theme.clone(), &id).with_icon("xrandr"),
                id: id,
                update_interval: Duration::new(get_u64_default!(config, "interval", 5), 0),
                current_idx: 0,
                icons: get_bool_default!(config, "icons", true),
                resolution: get_bool_default!(config, "resolution", false),
                step_width: step_width,
                monitors: Vec::new(),
                theme: theme,
            }
        }
    }

    fn get_active_monitors() -> Option<Vec<String>> {
        let active_montiors_cli = String::from_utf8(
            Command::new("sh")
                .args(&["-c", "xrandr --listactivemonitors | grep \\/"])
                .output().expect("There was a problem collecting active xrandr monitors.")
                .stdout)
            .expect("There was a problem while parsing xrandr monitors.");
        let monitors: Vec<&str> = active_montiors_cli.split('\n').collect();
        let mut active_monitors: Vec<String> = Vec::new();
        for monitor in monitors {
            if let Some((name, _)) = monitor.split_whitespace()
                                            .collect::<Vec<&str>>()
                                            .split_last() {
                active_monitors.push(String::from(*name));
            }
        }
        if !active_monitors.is_empty() {
            return Some(active_monitors);
        }
        None
    }

    fn get_monitor_metrics(monitor_names: &Vec<String>) -> Option<Vec<Monitor>> {
        let mut monitor_metrics: Vec<Monitor> = Vec::new();
        let grep_arg = format!("xrandr --verbose | grep -w '{} connected\\|Brightness'",
                               monitor_names.join(" connected\\|"));
        let monitor_info_cli = String::from_utf8(
            Command::new("sh")
                .args(&["-c", grep_arg.as_str()])
                .output().expect("There was a problem collecting monitor info.")
                .stdout)
            .expect("There was a problem while parsing monitor info.");

        let monitor_infos: Vec<&str> = monitor_info_cli.split('\n').collect();
        for i in 0..monitor_infos.len() {
            if i % 2 == 1 {
                continue;
            }
            let mut brightness = 0;
            let mut display: &str = "";
            let mi_line = unwrap_or_continue!(monitor_infos.get(i));
            let b_line = unwrap_or_continue!(monitor_infos.get(i + 1));
            let mi_line_args: Vec<&str> = mi_line.split_whitespace().collect();
            if let Some(name) = mi_line_args.get(0) {
                display = name.trim();
                if let Some(brightness_raw) = b_line.split(':')
                                                    .collect::<Vec<&str>>()
                                                    .get(1) {
                    brightness = (f32::from_str(brightness_raw.trim())
                                      .expect("Unable to parse brightness string to int.") * 100.0)
                                 .floor() as u32;
                }
            }
            if let Some(mut res) = mi_line_args.get(2) {
                if res.find('+').is_none() {
                    res = unwrap_or_continue!(mi_line_args.get(3));
                }
                if let Some(resolution) = res.split('+')
                                             .collect::<Vec<&str>>()
                                             .get(0) {
                    monitor_metrics.push(Monitor::new(display,
                                                      brightness,
                                                      resolution.trim()));
                }
            }
        }
        if !monitor_metrics.is_empty() {
            return Some(monitor_metrics);
        }
        None
    }

    fn display(&mut self) {
        if let Some(m) = self.monitors.get(self.current_idx) {
            let brightness_str = m.brightness.to_string();
            let values = map!("{display}" => m.name.clone(),
                              "{brightness}" => brightness_str,
                              "{resolution}" => m.resolution.clone());

            self.text.set_icon("xrandr");
            let format_str: &str;
            if self.resolution {
                if self.icons {
                    format_str = "{display} \u{f185} {brightness} \u{f096} {resolution}";
                } else {
                    format_str = "{display}: {brightness} [{resolution}]";
                }
            } else {
                if self.icons {
                    format_str = "{display} \u{f185} {brightness}";
                } else {
                    format_str = "{display}: {brightness}";
                }
            }

            if let Ok(fmt_template) = FormatTemplate::from_string(String::from(format_str)) {
                self.text.set_text(fmt_template.render_static_str(&values));
            }
        }
    }
}

impl Block for Xrandr
{
    fn update(&mut self) -> Option<Duration> {
        if let Some(am) = Xrandr::get_active_monitors() {
            if let Some(mm) = Xrandr::get_monitor_metrics(&am) {
                self.monitors = mm;
                self.display();
            }
        }
        Some(self.update_interval.clone())
    }

    fn view(&self) -> Vec<&I3BarWidget> {
        vec![&self.text]
    }

    fn click(&mut self, e: &I3BarEvent) {
        if let Some(ref name) = e.name {
            if name.as_str() == self.id {
                match e.button {
                    MouseButton::Left => {
                        if self.current_idx < self.monitors.len() - 1 {
                            self.current_idx += 1;
                        } else {
                            self.current_idx = 0;
                        }
                    },
                    MouseButton::WheelUp => {
                        if let Some(monitor) = self.monitors.get_mut(self.current_idx) {
                            if monitor.brightness <= (100 - self.step_width) {
                                monitor.set_brightness(self.step_width as i32);
                            }
                        }
                    },
                    MouseButton::WheelDown => {
                        if let Some(monitor) = self.monitors.get_mut(self.current_idx) {
                            if monitor.brightness >= self.step_width {
                                monitor.set_brightness(- (self.step_width as i32));
                            }
                        }
                    }
                    _ => {}
                }
                self.display();
            }
        }
    }

    fn id(&self) -> &str {
        &self.id
    }
}

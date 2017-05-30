use std::time::Duration;
use std::process::Command;
use std::sync::mpsc::Sender;
use std::str::FromStr;

use util::FormatTemplate;

use block::Block;
use widgets::text::TextWidget;
use widget::I3BarWidget;
use input::I3barEvent;
use scheduler::Task;

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
}

pub struct Xrandr {
    text: TextWidget,
    id: String,
    update_interval: Duration,
    monitors: Vec<Monitor>,
    icons: bool,
    resolution: bool,
    current_idx: usize,

    //useful, but optional
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
            Xrandr {
                text: TextWidget::new(theme.clone()).with_text("Template"),
                id: Uuid::new_v4().simple().to_string(),
                update_interval: Duration::new(get_u64_default!(config, "interval", 5), 0),
                current_idx: 0,
                icons: get_bool_default!(config, "icons", true),
                resolution: get_bool_default!(config, "resolution", false),
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
        if active_monitors.is_empty() {
            None
        } else {
            Some(active_monitors)
        }
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
                                                    .collect::<Vec<&str>>().get(1) {
                    brightness = (f32::from_str(brightness_raw.trim())
                                        .expect("Unable to parse brightness string to int.") * 100.0)
                                    .floor() as u32;
                }
            }
            if let Some(res) = mi_line_args.get(2) {
                if let Some(resolution) = res.split('+')
                                          .collect::<Vec<&str>>().get(0) {
                    monitor_metrics.push(Monitor::new(display,
                                                      brightness,
                                                      resolution.trim()));
                }
            }
        }
        if monitor_metrics.is_empty() {
            return None;
        } else {
            return Some(monitor_metrics);
        }
    }
}

impl Block for Xrandr
{
    fn update(&mut self) -> Option<Duration> {
        if let Some(am) = Xrandr::get_active_monitors() {
            if let Some(mm) = Xrandr::get_monitor_metrics(&am) {
                self.monitors = mm;
                if let Some(m) = self.monitors.get(self.current_idx) {
                    let brightness_str = m.brightness.to_string();
                    let values = map!("{display}" => m.name.clone(),
                                      "{brightness}" => brightness_str,
                                      "{resolution}" => m.resolution.clone());

                    self.text.set_icon("xrandr");
                    let mut format_str: &str = "";
                    if self.resolution {
                        if self.icons {
                            format_str = "{display} \u{f185} {brightness} \u{f096} {resolution}";
                        } else {
                            format_str = "{display}: {brightness} [{resolution}]";
                        }
                    } else {
                        if self.icons {
                            format_str = "{display} \u{f185}{brightness}";
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
        Some(self.update_interval.clone())
    }
    fn view(&self) -> Vec<&I3BarWidget> {
        vec![&self.text]
    }
    fn click(&mut self, _: &I3barEvent) {
        if self.current_idx < self.monitors.len() - 1 {
            self.current_idx += 1;
        } else {
            self.current_idx = 0;
        }
    }
    fn id(&self) -> &str {
        &self.id
    }
}

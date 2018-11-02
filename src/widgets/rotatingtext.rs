use config::Config;
use errors::*;
use std::time::{Duration, Instant};
use widget::{I3BarWidget, State};
use serde_json::value::Value;

#[derive(Clone, Debug)]
pub struct RotatingTextWidget {
    rotation_pos: usize,
    width: usize,
    rotation_interval: Duration,
    rotation_speed: Duration,
    next_rotation: Option<Instant>,
    content: String,
    icon: Option<String>,
    state: State,
    rendered: Value,
    cached_output: Option<String>,
    config: Config,
    pub rotating: bool,
}

#[allow(dead_code)]
impl RotatingTextWidget {
    pub fn new(interval: Duration, speed: Duration, width: usize, config: Config) -> RotatingTextWidget {
        RotatingTextWidget {
            rotation_pos: 0,
            width,
            rotation_interval: interval,
            rotation_speed: speed,
            next_rotation: None,
            content: String::new(),
            icon: None,
            state: State::Idle,
            rendered: json!({
                "full_text": "",
                "separator": false,
                "separator_block_width": 0,
                "background": "#000000",
                "color": "#000000"
            }),
            cached_output: None,
            config,
            rotating: false,
        }
    }

    pub fn with_icon(mut self, name: &str) -> Self {
        self.icon = self.config.icons.get(name).cloned();
        self.update();
        self
    }

    pub fn with_state(mut self, state: State) -> Self {
        self.state = state;
        self.update();
        self
    }

    pub fn with_text(mut self, content: &str) -> Self {
        self.content = String::from(content);
        self.rotation_pos = 0;
        if self.content.len() > self.width {
            self.next_rotation = Some(Instant::now() + self.rotation_interval);
        } else {
            self.next_rotation = None;
        }
        self.update();
        self
    }

    pub fn set_state(&mut self, state: State) {
        self.state = state;
        self.update();
    }

    pub fn set_icon(&mut self, name: &str) {
        self.icon = self.config.icons.get(name).cloned();
        self.update();
    }

    pub fn set_text(&mut self, content: String) {
        if self.content != content {
            self.content = content;
            self.rotation_pos = 0;
            if self.content.len() > self.width {
                self.next_rotation = Some(Instant::now() + self.rotation_interval);
            } else {
                self.next_rotation = None;
            }
        }
        self.update()
    }

    fn get_rotated_content(&self) -> String {
        if self.content.len() > self.width {
            let missing = (self.rotation_pos + self.width).saturating_sub(self.content.len());
            if missing == 0 {
                self.content
                    .chars()
                    .skip(self.rotation_pos)
                    .take(self.width)
                    .collect()
            } else {
                let mut avail: String = self.content
                    .chars()
                    .skip(self.rotation_pos)
                    .take(self.width)
                    .collect();
                avail.push_str("|");
                avail.push_str(&self.content.chars().take(missing - 1).collect::<String>());
                avail
            }

        } else {
            self.content.clone()
        }
    }

    fn update(&mut self) {
        let (key_bg, key_fg) = self.state.theme_keys(&self.config.theme);

        self.rendered = json!({
            "full_text": format!("{}{} ",
                                self.icon.clone().unwrap_or_else(|| String::from(" ")),
                                self.get_rotated_content()),
            "separator": false,
            "separator_block_width": 0,
            "min_width": if self.content == "" {"".to_string()} else {"0".repeat(self.width+5)},
            "align": "left",
            "background": key_bg,
            "color": key_fg
        });

        self.cached_output = Some(self.rendered.to_string());
    }

    pub fn next(&mut self) -> Result<(bool, Option<Duration>)> {
        if let Some(next_rotation) = self.next_rotation {
            let now = Instant::now();
            if next_rotation > now {
                Ok((false, Some(next_rotation - now)))
            } else if self.rotating {
                if self.rotation_pos < self.content.len() {
                    self.rotation_pos += 1;
                    self.next_rotation = Some(now + self.rotation_speed);
                    self.update();
                    Ok((true, Some(self.rotation_speed)))
                } else {
                    self.rotation_pos = 0;
                    self.rotating = false;
                    self.next_rotation = Some(now + self.rotation_interval);
                    self.update();
                    Ok((true, Some(self.rotation_interval)))
                }
            } else {
                self.rotating = true;
                Ok((true, Some(self.rotation_speed)))
            }
        } else {
            Ok((false, None))
        }
    }
}

impl I3BarWidget for RotatingTextWidget {
    fn to_string(&self) -> String {
        self.cached_output
            .clone()
            .unwrap_or_else(|| self.rendered.to_string())
    }

    fn get_rendered(&self) -> &Value {
        &self.rendered
    }
}

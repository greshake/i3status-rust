use std::time::{Duration, Instant};

use serde_json::value::Value;

use crate::config::Config;
use crate::errors::*;
use crate::widget::{I3BarWidget, Spacing, State};

#[derive(Clone, Debug)]
pub struct RotatingTextWidget {
    id: usize,
    rotation_pos: usize,
    max_width: usize,
    dynamic_width: bool,
    rotation_interval: Duration,
    rotation_speed: Duration,
    next_rotation: Option<Instant>,
    content: String,
    icon: Option<String>,
    state: State,
    spacing: Spacing,
    rendered: Value,
    cached_output: Option<String>,
    config: Config,
    pub rotating: bool,
}

#[allow(dead_code)]
impl RotatingTextWidget {
    pub fn new(
        interval: Duration,
        speed: Duration,
        max_width: usize,
        dynamic_width: bool,
        config: Config,
        id: usize,
    ) -> RotatingTextWidget {
        RotatingTextWidget {
            id,
            rotation_pos: 0,
            max_width,
            dynamic_width,
            rotation_interval: interval,
            rotation_speed: speed,
            next_rotation: None,
            content: String::new(),
            icon: None,
            state: State::Idle,
            spacing: Spacing::Normal,
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

    pub fn with_spacing(mut self, spacing: Spacing) -> Self {
        self.spacing = spacing;
        self.update();
        self
    }

    pub fn with_text(mut self, content: &str) -> Self {
        self.content = String::from(content);
        self.rotation_pos = 0;
        if self.content.chars().count() > self.max_width {
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
            if self.content.chars().count() > self.max_width {
                self.next_rotation = Some(Instant::now() + self.rotation_interval);
            } else {
                self.next_rotation = None;
            }
        }
        self.update()
    }

    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
    }

    fn get_rotated_content(&self) -> String {
        if self.content.chars().count() > self.max_width {
            let missing =
                (self.rotation_pos + self.max_width).saturating_sub(self.content.chars().count());
            if missing == 0 {
                self.content
                    .chars()
                    .skip(self.rotation_pos)
                    .take(self.max_width)
                    .collect()
            } else {
                let mut avail: String = self
                    .content
                    .chars()
                    .skip(self.rotation_pos)
                    .take(self.max_width)
                    .collect();
                avail.push('|');
                avail.push_str(&self.content.chars().take(missing - 1).collect::<String>());
                avail
            }
        } else {
            self.content.clone()
        }
    }

    fn update(&mut self) {
        let (key_bg, key_fg) = self.state.theme_keys(&self.config.theme);

        let icon = self.icon.clone().unwrap_or_else(|| match self.spacing {
            Spacing::Normal => String::from(" "),
            _ => String::from(""),
        });

        self.rendered = json!({
            "full_text": format!("{}{}{}",
                                icon,
                                self.get_rotated_content(),
                                match self.spacing {
                                    Spacing::Hidden => String::from(""),
                                    _ => String::from(" ")
                                }),
            "separator": false,
            "separator_block_width": 0,
            "name" : self.id.to_string(),
            "min_width":
                if self.content.is_empty() {
                    "".to_string()
                } else {
                    let text_width = self.get_rotated_content().chars().count();
                    let icon_width = icon.chars().count();
                    if self.dynamic_width && text_width < self.max_width {
                        "0".repeat(text_width + icon_width)
                    } else {
                        "0".repeat(self.max_width + icon_width + 1)
                    }
                },
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
                if self.rotation_pos < self.content.chars().count() {
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

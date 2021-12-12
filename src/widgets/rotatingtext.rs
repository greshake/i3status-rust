use std::time::{Duration, Instant};

use super::{I3BarWidget, Spacing, State};
use crate::config::SharedConfig;
use crate::errors::*;
use crate::protocol::i3bar_block::{I3BarBlock, I3BarBlockMinWidth};
use crate::util::escape_pango_text;

#[derive(Clone, Debug)]
pub struct RotatingTextWidget {
    pub instance: usize,
    pub rotating: bool,
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
    shared_config: SharedConfig,
    inner: I3BarBlock,
}

#[allow(dead_code)]
impl RotatingTextWidget {
    pub fn new(
        id: usize,
        instance: usize,
        interval: Duration,
        speed: Duration,
        max_width: usize,
        dynamic_width: bool,
        shared_config: SharedConfig,
    ) -> RotatingTextWidget {
        let inner = I3BarBlock {
            name: Some(id.to_string()),
            instance: Some(instance.to_string()),
            ..I3BarBlock::default()
        };

        RotatingTextWidget {
            instance,
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
            //cached_output: None,
            shared_config,
            rotating: false,

            inner,
        }
    }

    pub fn with_icon(mut self, name: &str) -> Result<Self> {
        self.icon = Some(self.shared_config.get_icon(name)?);
        self.update();
        Ok(self)
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

    pub fn set_icon(&mut self, name: &str) -> Result<()> {
        self.icon = Some(self.shared_config.get_icon(name)?);
        self.update();
        Ok(())
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
        let (key_bg, key_fg) = self.state.theme_keys(&self.shared_config.theme);

        let mut icon = self.icon.clone().unwrap_or_else(|| match self.spacing {
            Spacing::Normal => String::from(" "),
            _ => String::from(""),
        });

        self.inner.full_text = format!(
            "{}{}{}",
            icon,
            escape_pango_text(&self.get_rotated_content()),
            match self.spacing {
                Spacing::Hidden => String::from(""),
                _ => String::from(" "),
            }
        );
        self.inner.min_width = {
            if self.dynamic_width || self.content.is_empty() {
                None
            } else {
                icon.push_str(&"0".repeat(self.max_width + 1));
                Some(I3BarBlockMinWidth::Text(icon))
            }
        };
        self.inner.background = key_bg;
        self.inner.color = key_fg;
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
    fn get_data(&self) -> I3BarBlock {
        self.inner.clone()
    }
}

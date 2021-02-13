use serde_json::value::Value;

use super::super::widget::I3BarWidget;
use crate::config::Config;
use crate::widget::Spacing;
use crate::widget::State;

#[derive(Clone, Debug)]
pub struct ButtonWidget {
    id: usize,
    content: Option<String>,
    icon: Option<String>,
    state: State,
    spacing: Spacing,
    rendered: Value,
    cached_output: Option<String>,
    config: Config,
}

impl ButtonWidget {
    pub fn new(config: Config, id: usize) -> Self {
        ButtonWidget {
            content: None,
            icon: None,
            state: State::Idle,
            spacing: Spacing::Normal,
            id,
            rendered: json!({
                "full_text": "",
                "separator": false,
                "separator_block_width": 0,
                "background": "#000000",
                "color": "#000000",
                "markup": "pango"
            }),
            config,
            cached_output: None,
        }
    }

    pub fn with_icon(mut self, name: &str) -> Self {
        self.icon = self.config.icons.get(name).cloned();
        self.update();
        self
    }

    pub fn with_content(mut self, content: Option<String>) -> Self {
        self.content = content;
        self.update();
        self
    }

    pub fn with_text(mut self, content: &str) -> Self {
        self.content = Some(String::from(content));
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

    pub fn set_text<S: Into<String>>(&mut self, content: S) {
        self.content = Some(content.into());
        self.update();
    }

    pub fn set_icon(&mut self, name: &str) {
        self.icon = self.config.icons.get(name).cloned();
        self.update();
    }

    pub fn set_state(&mut self, state: State) {
        self.state = state;
        self.update();
    }

    pub fn set_spacing(&mut self, spacing: Spacing) {
        self.spacing = spacing;
        self.update();
    }

    fn update(&mut self) {
        let (key_bg, key_fg) = self.state.theme_keys(&self.config.theme);

        // When rendered inline, remove the leading space
        self.rendered = json!({
            "full_text": format!("{}{}{}",
                                self.icon.clone().unwrap_or_else(|| {
                                    match self.spacing {
                                        Spacing::Normal => String::from(" "),
                                        _ => String::from("")
                                    }
                                }),
                                self.content.clone().unwrap_or_else(|| String::from("")),
                                match self.spacing {
                                    Spacing::Hidden => String::from(""),
                                    _ => String::from(" ")
                                }
                            ),
            "separator": false,
            "name": self.id.to_string(),
            "separator_block_width": 0,
            "background": key_bg,
            "color": key_fg,
            "markup": "pango"
        });

        self.cached_output = Some(self.rendered.to_string());
    }
}

impl I3BarWidget for ButtonWidget {
    fn to_string(&self) -> String {
        self.cached_output
            .clone()
            .unwrap_or_else(|| self.rendered.to_string())
    }

    fn get_rendered(&self) -> &Value {
        &self.rendered
    }
}

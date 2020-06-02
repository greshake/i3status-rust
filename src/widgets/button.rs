use serde_json::value::Value;

use super::super::widget::{I3BarWidget, WidgetWidth};
use crate::config::Config;
use crate::widget::Spacing;
use crate::widget::State;

#[derive(Clone, Debug)]
pub struct ButtonWidget {
    content: Option<String>,
    short_content: Option<String>,
    icon: Option<String>,
    state: State,
    spacing: Spacing,
    id: String,
    rendered: Value,
    cached_output: Option<String>,
    config: Config,
}

impl ButtonWidget {
    pub fn new(config: Config, id: &str) -> Self {
        ButtonWidget {
            content: None,
            short_content: None,
            icon: None,
            state: State::Idle,
            spacing: Spacing::Normal,
            id: String::from(id),
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
        if self.short_content == None {
            self.short_content = self.content.clone();
        }
        self.update();
        self
    }

    #[allow(dead_code)]
    pub fn with_short_content(mut self, content: Option<String>) -> Self {
        self.short_content = content;
        self.update();
        self
    }

    /// Set the `full_text` adn `short_text` representation of the widget
    pub fn with_text(mut self, content: &str) -> Self {
        self.content = Some(String::from(content));
        self.short_content = self.content.clone();
        self.update();
        self
    }

    /// Set the `short_text` representation of the widget. Shall be call after `with_text`
    /// to have any effect.
    pub fn with_short_text(mut self, content: &str) -> Self {
        self.short_content = Some(String::from(content));
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
        let content = Some(content.into());
        self.short_content = content.clone();
        self.content = content;
        self.update();
    }

    pub fn set_short_text<S: Into<String>>(&mut self, content: S) {
        self.short_content = Some(content.into());
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

    /// Set full_text and short_text accordingly to the width parameter
    /// WidgetWidth::Full forces both to content
    /// WidgetWidth::Short forces both to short_content
    pub fn set_text_with_width<S, T>(&mut self, width: &WidgetWidth, content: S, short_content: T)
    where
        T: Into<String> + Clone,
        S: Into<String> + Clone,
    {
        match width {
            WidgetWidth::Default => {
                self.set_text(content);
                self.set_short_text(short_content);
            }
            WidgetWidth::Short => {
                self.set_text(short_content.clone());
                self.set_short_text(short_content);
            }
            WidgetWidth::Full => {
                self.set_text(content.clone());
                self.set_short_text(content);
            }
        }
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
            "short_text": format!("{}{}{}",
                                self.icon.clone().unwrap_or_else(|| {
                                    match self.spacing {
                                        Spacing::Normal => String::from(" "),
                                        _ => String::from("")
                                    }
                                }),
                                self.short_content.clone().unwrap_or_else(|| String::from("")),
                                match self.spacing {
                                    Spacing::Hidden => String::from(""),
                                    _ => String::from(" ")
                                }
                            ),
            "separator": false,
            "name": self.id.clone(),
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

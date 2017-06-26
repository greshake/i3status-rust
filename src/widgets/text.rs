use config::Config;
use widget::State;
use serde_json::value::Value;
use super::super::widget::I3BarWidget;

#[derive(Clone, Debug)]
pub struct TextWidget {
    content: Option<String>,
    icon: Option<String>,
    state: State,
    rendered: Value,
    cached_output: Option<String>,
    config: Config,
}

impl TextWidget {
    pub fn new(config: Config) -> Self {
        TextWidget {
            content: None,
            icon: None,
            state: State::Idle,
            rendered: json!({
                "full_text": "",
                "separator": false,
                "separator_block_width": 0,
                "background": "#000000",
                "color": "#000000"
            }),
            config: config,
            cached_output: None,
        }
    }

    pub fn with_icon(mut self, name: &str) -> Self {
        self.icon = self.config.icons.get(name).cloned();
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

    pub fn set_text(&mut self, content: String) {
        self.content = Some(content);
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

    fn update(&mut self) {
        let (key_bg, key_fg) = self.state.theme_keys(&self.config.theme);

        self.rendered = json!({
            "full_text": format!("{}{} ",
                                self.icon.clone().unwrap_or(String::from(" ")),
                                self.content.clone().unwrap_or(String::from(""))),
            "separator": false,
            "separator_block_width": 0,
            "background": key_bg.to_owned(),
            "color": key_fg.to_owned()
        });

        self.cached_output = Some(self.rendered.to_string());
    }
}

impl I3BarWidget for TextWidget {
    fn to_string(&self) -> String {
        self.cached_output
            .clone()
            .unwrap_or(self.rendered.to_string())
    }

    fn get_rendered(&self) -> &Value {
        &self.rendered
    }
}

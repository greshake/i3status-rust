use num_traits::{clamp, ToPrimitive};
use serde_json::value::Value;

use super::super::widget::I3BarWidget;
use crate::config::Config;
use crate::widget::Spacing;
use crate::widget::State;

#[derive(Clone, Debug)]
pub struct GraphWidget {
    id: usize,
    content: Option<String>,
    icon: Option<String>,
    state: State,
    spacing: Spacing,
    rendered: Value,
    cached_output: Option<String>,
    config: Config,
}
#[allow(dead_code)]
impl GraphWidget {
    pub fn new(config: Config, id: usize) -> Self {
        GraphWidget {
            id,
            content: None,
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
            config,
            cached_output: None,
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

    pub fn set_values<T>(&mut self, content: &[T], min: Option<T>, max: Option<T>)
    where
        T: Ord + ToPrimitive,
    {
        let bars = ["▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
        let min: f64 = match min {
            Some(x) => x.to_f64().unwrap(),
            None => content.iter().min().unwrap().to_f64().unwrap(),
        };
        let max: f64 = match max {
            Some(x) => x.to_f64().unwrap(),
            None => content.iter().max().unwrap().to_f64().unwrap(),
        };
        let extant = max - min;
        if extant.is_normal() {
            let length = bars.len() as f64 - 1.0;
            let bar = content
                .iter()
                .map(|x| {
                    bars[((clamp(x.to_f64().unwrap(), min, max) - min) / extant * length) as usize]
                })
                .collect::<Vec<&'static str>>()
                .concat();
            self.content = Some(bar);
        } else {
            let bar = (0..content.len() - 1)
                .map(|_| bars[0])
                .collect::<Vec<&'static str>>()
                .concat();
            self.content = Some(bar);
        }
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
            "separator_block_width": 0,
            "name": self.id.to_string(),
            "background": key_bg.to_owned(),
            "color": key_fg.to_owned()
        });

        self.cached_output = Some(self.rendered.to_string());
    }
}

impl I3BarWidget for GraphWidget {
    fn to_string(&self) -> String {
        self.cached_output
            .clone()
            .unwrap_or_else(|| self.rendered.to_string())
    }

    fn get_rendered(&self) -> &Value {
        &self.rendered
    }
}

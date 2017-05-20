use std::time::Duration;
use std::process::Command;
use std::error::Error;

use block::Block;
use widgets::text::TextWidget;
use widget::{I3BarWidget, State};
use input::I3barEvent;

use serde_json::Value;
use uuid::Uuid;

pub struct Script {
    file: String,
    id: String,
    update_interval: Duration,
    output: TextWidget,
}

impl Script {
    pub fn new(config: Value, theme: Value) -> Script {
        Script {
            id: Uuid::new_v4().simple().to_string(),
            file: get_str!(config, "file"),
            update_interval: Duration::new(get_u64_default!(config, "interval", 10), 0),
            output: TextWidget::new(theme),
        }
    }
}

impl Block for Script {
    fn update(&mut self) -> Option<Duration> {
        let output = Command::new(&self.file)
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_else(|e| e.description().to_owned());
        self.output.set_text(output);

        Some(self.update_interval.clone())
    }

    fn view(&self) -> Vec<&I3BarWidget> {
        vec![&self.output]
    }

    fn click(&mut self, _: &I3barEvent) {}

    fn id(&self) -> &str {
        &self.id
    }
}

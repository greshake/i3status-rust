use std::time::Duration;
use std::process::Command;
use std::error::Error;
use std::iter::{Cycle, Peekable};
use std::vec;

use block::Block;
use widgets::text::TextWidget;
use widget::I3BarWidget;
use input::I3barEvent;

use serde_json::Value;
use uuid::Uuid;

pub struct Script {
    id: String,
    update_interval: Duration,
    output: TextWidget,
    command: Option<String>,
    on_click: Option<String>,
    cycle: Option<Peekable<Cycle<vec::IntoIter<String>>>>,
}

impl Script {
    pub fn new(config: Value, theme: Value) -> Script {
        let mut script = Script {
            id: Uuid::new_v4().simple().to_string(),
            update_interval: Duration::new(get_u64_default!(config, "interval", 10), 0),
            output: TextWidget::new(theme),
            command: None,
            on_click: None,
            cycle: None,
        };

        if let Some(cycle) = config["cycle"].as_array() {
            script.cycle = Some(cycle.into_iter()
                                .map(|s| s.as_str().expect("'cycle' should be an array of strings").to_string())
                                // TODO: find a simple way to avoid collect
                                .collect::<Vec<_>>()
                                .into_iter()
                                .cycle()
                                .peekable());
            return script
        };

        if let Some(command) = config["command"].as_str() {
            script.command = Some(command.to_string())
        };

        if let Some(on_click) = config["on_click"].as_str() {
            script.on_click = Some(on_click.to_string())
        };

        script
    }
}

impl Block for Script {
    fn update(&mut self) -> Option<Duration> {
        let command_str = self
            .cycle
            .as_mut()
            .map(|c| c.peek().cloned().unwrap_or("".to_owned()))
            .or(self.command.clone())
            .unwrap_or("".to_owned());

        let output = Command::new("sh")
            .args(&["-c", &command_str])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
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

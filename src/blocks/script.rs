use std::time::Duration;
use std::process::Command;
use std::error::Error;

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
    command: String,
    args: Vec<String>,
}

impl Script {
    pub fn new(config: Value, theme: Value) -> Script {
        let mut command = config["command"]
            .as_array()
            .map(|arr| {
                arr.into_iter()
                    .map(|e| e.as_str().map(String::from).expect("'command' should be an array of strings"))
                    .collect::<Vec<_>>()
            })
            .or(config["command"].as_str().map(|s| s.split_whitespace().map(|str| str.to_owned()).collect()))
            .expect("'command' should be a string or an array of string")
            .into_iter();

        Script {
            id: Uuid::new_v4().simple().to_string(),
            update_interval: Duration::new(get_u64_default!(config, "interval", 10), 0),
            output: TextWidget::new(theme),
            command: command.next().expect("no command provided"),
            args: command.collect::<Vec<_>>(),
        }
    }
}

impl Block for Script {
    fn update(&mut self) -> Option<Duration> {
        let output = Command::new(self.command.clone())
            .args(&self.args)
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

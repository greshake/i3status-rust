use std::time::{Duration, Instant};
use std::process::Command;
use std::error::Error;
use std::iter::{Cycle, Peekable};
use std::vec;
use std::sync::mpsc::Sender;

use block::Block;
use widgets::button::ButtonWidget;
use widget::I3BarWidget;
use input::I3barEvent;
use scheduler::Task;

use serde_json::Value;
use uuid::Uuid;

pub struct Custom {
    id: String,
    update_interval: Duration,
    output: ButtonWidget,
    command: Option<String>,
    on_click: Option<String>,
    cycle: Option<Peekable<Cycle<vec::IntoIter<String>>>>,
    tx_update_request: Sender<Task>,
}

impl Custom {
    pub fn new(config: Value, tx: Sender<Task>, theme: Value) -> Custom {
        let mut custom = Custom {
            id: Uuid::new_v4().simple().to_string(),
            update_interval: Duration::new(get_u64_default!(config, "interval", 10), 0),
            output: ButtonWidget::new(theme.clone(), ""),
            command: None,
            on_click: None,
            cycle: None,
            tx_update_request: tx,
        };
        custom.output = ButtonWidget::new(theme, &custom.id);

        if let Some(on_click) = config["on_click"].as_str() {
            custom.on_click = Some(on_click.to_string())
        };

        if let Some(cycle) = config["cycle"].as_array() {
            custom.cycle = Some(cycle.into_iter()
                                .map(|s| s.as_str().expect("'cycle' should be an array of strings").to_string())
                                .collect::<Vec<_>>()
                                .into_iter()
                                .cycle()
                                .peekable());
            return custom
        };

        if let Some(command) = config["command"].as_str() {
            custom.command = Some(command.to_string())
        };

        custom
    }
}

impl Block for Custom {
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

    fn click_left(&mut self, event: &I3barEvent) {
        if let Some(ref name) = event.name {
            if name != &self.id {
                return
            }
        } else {
            return
        }

        let mut update = false;

        if let Some(ref on_click) = self.on_click {
            Command::new("sh")
                .args(&["-c", on_click])
                .output().ok();
            update = true;
        }

        if let Some(ref mut cycle) = self.cycle {
            cycle.next();
            update = true;
        }

        if update {
            self.tx_update_request.send(Task { id: self.id.clone(), update_time: Instant::now() }).ok();
        }
    }

    fn id(&self) -> &str {
        &self.id
    }
}

use std::time::{Duration, Instant};
use std::process::Command;
use std::error::Error;
use std::iter::{Cycle, Peekable};
use std::vec;
use std::sync::mpsc::Sender;

use block::Block;
use config::Config;
use widgets::button::ButtonWidget;
use widget::I3BarWidget;
use input::I3BarEvent;
use scheduler::Task;

use toml::value::Value;
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

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct CustomConfig {
    /// Update interval in seconds
    #[serde(default = "CustomConfig::default_interval")]
    pub interval: Duration,

    /// Shell Command to execute & display
    pub command: Option<String>,

    /// Command to execute when the button is clicked
    pub on_click: Option<String>,

    /// Commands to execute and change when the button is clicked
    pub cycle: Option<Vec<String>>,
}

impl CustomConfig {
    fn default_interval() -> Duration {
        Duration::from_secs(10)
    }
}

impl Custom {
    pub fn new(block_config: Value, config: Config, tx: Sender<Task>) -> Custom {
        let mut custom = Custom {
            id: Uuid::new_v4().simple().to_string(),
            update_interval: Duration::new(get_u64_default!(block_config, "interval", 10), 0),
            output: ButtonWidget::new(config.clone(), ""),
            command: None,
            on_click: None,
            cycle: None,
            tx_update_request: tx,
        };
        custom.output = ButtonWidget::new(config, &custom.id);

        if let Some(on_click) = block_config.get("on_click").and_then(|s| s.as_str()) {
            custom.on_click = Some(on_click.to_string())
        };

        if let Some(cycle) = block_config.get("cycle").and_then(|s| s.as_array()) {
            custom.cycle = Some(cycle.into_iter()
                                .map(|s| s.as_str().expect("'cycle' should be an array of strings").to_string())
                                .collect::<Vec<_>>()
                                .into_iter()
                                .cycle()
                                .peekable());
            return custom
        };

        if let Some(command) = block_config.get("command").and_then(|s| s.as_str()) {
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

    fn click(&mut self, event: &I3BarEvent) {
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

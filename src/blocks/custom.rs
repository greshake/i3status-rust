use chan::Sender;
use std::env;
use std::iter::{Cycle, Peekable};
use std::process::Command;
use std::time::{Duration, Instant};
use std::vec;

use block::{Block, ConfigBlock};
use config::Config;
use de::deserialize_duration;
use errors::*;
use input::{I3BarEvent, MouseButton};
use scheduler::Task;
use widget::I3BarWidget;
use widgets::button::ButtonWidget;

use uuid::Uuid;

pub struct Custom {
    id: String,
    update_interval: Duration,
    output: ButtonWidget,
    command: Option<String>,
    on_click: Option<String>,
    on_set_clicks: Option<Vec<MouseAction>>,
    cycle: Option<Peekable<Cycle<vec::IntoIter<String>>>>,
    tx_update_request: Sender<Task>,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct CustomConfig {
    /// Update interval in seconds
    #[serde(default = "CustomConfig::default_interval", deserialize_with = "deserialize_duration")]
    pub interval: Duration,

    /// Shell Command to execute & display
    pub command: Option<String>,

    /// Command to execute when the button is clicked
    /// **This will be run on _EVERY_ type of mouse click**
    pub on_click: Option<String>,

    /// Commands to execute when their specified button is clicked
    pub on_set_clicks: Option<Vec<MouseAction>>,

    /// Commands to execute and change when any mouse button is clicked
    pub cycle: Option<Vec<String>>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct MouseAction {
    pub button: MouseButton,
    pub action: String,
}

impl CustomConfig {
    fn default_interval() -> Duration {
        Duration::from_secs(10)
    }
}

impl ConfigBlock for Custom {
    type Config = CustomConfig;

    fn new(block_config: Self::Config, config: Config, tx: Sender<Task>) -> Result<Self> {
        let id = Uuid::new_v4().simple().to_string();

        Ok(Custom {
            output: ButtonWidget::new(config, &id),
            id,
            update_interval: block_config.interval,
            on_click: block_config.on_click,
            on_set_clicks: block_config.on_set_clicks,
            command: if block_config.cycle.is_none() { block_config.command } else { None },
            cycle: block_config.cycle.map(|cycle| cycle.into_iter().cycle().peekable()),
            tx_update_request: tx,
        })
    }
}

impl Block for Custom {
    fn update(&mut self) -> Result<Option<Duration>> {
        let command_str = self
            .cycle
            .as_mut()
            .map(|c| c.peek().cloned().unwrap_or(String::new()))
            .or(self.command.clone())
            .unwrap_or(String::new());

        let output = Command::new(env::var("SHELL").unwrap_or("sh".to_owned()))
            .args(&["-c", &command_str])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
            .unwrap_or_else(|e| e.description().to_owned());

        self.output.set_text(output);

        Ok(Some(self.update_interval))
    }

    fn view(&self) -> Vec<&I3BarWidget> {
        vec![&self.output]
    }

    fn click(&mut self, event: &I3BarEvent) -> Result<()> {
        if let Some(ref name) = event.name {
            if name == &self.id {
                let mut update = false;

                if let Some(ref on_click) = self.on_click {
                    Command::new(env::var("SHELL").unwrap_or("sh".to_owned())).args(&["-c", on_click]).output().ok();
                    update = true;
                }

                if let Some(ref possible_clicks) = self.on_set_clicks {
                    for ma in possible_clicks.iter().filter(|ma| ma.button == event.button) {
                        Command::new(env::var("SHELL").unwrap_or("sh".to_owned())).args(&["-c", &ma.action]).output().ok();
                        update = true;
                    }
                }

                if let Some(ref mut cycle) = self.cycle {
                    cycle.next();
                    update = true;
                }

                if update {
                    self.tx_update_request.send(Task {
                        id: self.id.clone(),
                        update_time: Instant::now(),
                    });
                }
            }
        }

        Ok(())
    }

    fn id(&self) -> &str {
        &self.id
    }
}

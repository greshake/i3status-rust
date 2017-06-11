use std::time::Duration;
use std::process::Command;
use std::error::Error;

use block::Block;
use config::Config;
use widgets::button::ButtonWidget;
use widget::I3BarWidget;
use input::I3BarEvent;

use toml::value::Value;
use uuid::Uuid;


pub struct Toggle {
    text: ButtonWidget,
    command_on: String,
    command_off: String,
    command_state: String,
    update_interval: Option<Duration>,
    toggled: bool,
    id: String,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct ToggleConfig {
    /// Update interval in seconds
    pub interval: Option<Duration>,

    /// Shell Command to enable the toggle
    pub command_on: Option<String>,

    /// Shell Command to disable the toggle
    pub command_off: Option<String>,

    /// Shell Command to determine toggle state. <br/>Empty output => off. Any output => on.
    pub command_state: Option<String>,
}

impl Toggle {
    pub fn new(block_config: Value, config: Config) -> Toggle {
        let id = Uuid::new_v4().simple().to_string();
        let interval = get_f64_default!(block_config, "interval", -1.);
        Toggle {
            text: ButtonWidget::new(config, &id)
                .with_text(&get_str!(block_config, "text")),
            command_on: get_str!(block_config, "command_on"),
            command_off: get_str!(block_config, "command_off"),
            command_state: get_str!(block_config, "command_state"),
            id,
            toggled: false,
            update_interval: if interval < 0.
                {None} else
            {Some(Duration::new(interval as u64, 0))},
        }
    }
}


impl Block for Toggle
{
    fn update(&mut self) -> Option<Duration> {
        let output = Command::new("sh")
            .args(&["-c", &self.command_state])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
            .unwrap_or_else(|e| e.description().to_owned());

        self.text.set_icon(match output.trim_left() {
            "" => {self.toggled = false; "toggle_off"},
            _ => {self.toggled = true; "toggle_on"}
        });

        self.update_interval
    }
    fn view(&self) -> Vec<&I3BarWidget> {
        vec![&self.text]
    }
    fn click(&mut self, e: &I3BarEvent) {
        if let Some(ref name) = e.name {
            if name.as_str() == self.id {
                let cmd = match self.toggled {
                    true => {
                        self.toggled = false;
                        self.text.set_icon("toggle_off");
                        &self.command_off
                    },
                    false => {
                        self.toggled = true;
                        self.text.set_icon("toggle_on");
                        &self.command_on
                    }
                };

                Command::new("sh")
                    .args(&["-c", cmd])
                    .output().ok();
            }
        }
    }
    fn id(&self) -> &str {
        &self.id
    }
}

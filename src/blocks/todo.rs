use block::{Block, ConfigBlock};
use chan::Sender;
use config::Config;
use de::deserialize_duration;
use errors::*;
use input::{I3BarEvent, MouseButton};
use scheduler::Task;

use std::fs::File;
use std::io::BufReader;
use std::io::prelude::*;
use std::path::Path;
use std::time::{Duration, Instant};

use util::FormatTemplate;
use uuid::Uuid;
use widgets::button::ButtonWidget;
use widget::{I3BarWidget, State};

extern crate shellexpand;

pub struct Todo {
    output: ButtonWidget,
    collapsed: bool,
    actual_line: usize,
    id: String,
    update_interval: Duration,
    filename: String,
    format: FormatTemplate,
    is_idle: bool,
    minimum_info: usize,
    minimum_warning: usize,
    minimum_critical: usize,
    tx_update_request: Sender<Task>,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct TodoConfig {
    /// Update interval in seconds
    #[serde(default = "TodoConfig::default_interval", deserialize_with = "deserialize_duration")]
    pub interval: Duration,

    /// Todo file location
    #[serde(default = "TodoConfig::default_filename")]
    pub filename: String,

    /// Format string
    #[serde(default = "TodoConfig::default_format")]
    pub format: String,

    /// Collapsed by default?
    #[serde(default = "TodoConfig::default_collapsed")]
    pub collapsed: bool,

    /// Idle the display
    #[serde(default = "TodoConfig::default_idle")]
    pub idle: bool,

    /// Minimum number, where state is set to info
    #[serde(default = "TodoConfig::default_info")]
    pub info: usize,

    /// Minimum number, where state is set to warning
    #[serde(default = "TodoConfig::default_warning")]
    pub warning: usize,

    /// Minimum number, where state is set to critical
    #[serde(default = "TodoConfig::default_critical")]
    pub critical: usize,
}

impl TodoConfig {
    fn default_interval() -> Duration {
        Duration::from_secs(5)
    }

    fn default_filename() -> String {
        shellexpand::tilde("~/Documents/todo.txt").to_string()
    }

    fn default_format() -> String {
        "{size} - ({number}) '{text}'".to_string()
    }

    fn default_collapsed() -> bool {
        false
    }

    fn default_idle() -> bool {
        false
    }

    fn default_info() -> usize {
        4
    }

    fn default_warning() -> usize {
        15
    }

    fn default_critical() -> usize {
        50
    }
}

impl ConfigBlock for Todo {
    type Config = TodoConfig;

    fn new(block_config: Self::Config, config: Config, tx: Sender<Task>) -> Result<Self> {
        let i = Uuid::new_v4().simple().to_string();
        Ok(Todo {
            id: i.clone(),
            output: ButtonWidget::new(config, i.as_str())
                .with_icon("todo"),
            update_interval: block_config.interval,
            filename: shellexpand::tilde(&block_config.filename).to_string(),
            format: FormatTemplate::from_string(block_config.format)
                .block_error("todo", "Invalid format specified for todo")?,
            collapsed: block_config.collapsed,
            actual_line: 0,
            is_idle: block_config.idle,
            minimum_info: block_config.info,
            minimum_warning: block_config.warning,
            minimum_critical: block_config.critical,
            tx_update_request: tx,
        })
    }
}

impl Block for Todo {
    fn update(&mut self) -> Result<Option<Duration>> {
        if !Path::new(&self.filename).exists() {
            let _f = File::create(&self.filename);
        }

        let file = File::open(&self.filename)
            .block_error("todo", &format!("failed to read {}", self.filename))?;
        let buf_reader = BufReader::new(file);
        let mut lines: Vec<String> = buf_reader.lines()
            .map(|l| l.expect("Could not parse line"))
            .collect();
        lines.retain(|ref x| !x.is_empty());

        self.actual_line = match self.actual_line {
            x if x == usize::max_value() => lines.len() - 1,
            x if x >= lines.len() => 0,
            _ => self.actual_line,
        };

        let values = map!("{size}" => lines.len().to_string(),
                          "{number}" => {
                              if lines.len() == 0 {
                                  "-".to_string()
                              } else {
                                  (self.actual_line + 1).to_string()
                              }},
                          "{text}" => {
                              if lines.len() == 0 {
                                  format!("<{} EMPTY>", self.filename)
                              } else {
                                  lines[self.actual_line].clone()
                              }}
                         );


        if self.collapsed {
            self.output.set_text(format!("{}", lines.len()));
        } else {
            self.output.set_text(self.format.render_static_str(&values)?);
        }

        self.output.set_icon("todo");
        self.output.set_state(if self.is_idle {
            State::Idle
        } else {
            match lines.len() {
                x if x > self.minimum_critical => State::Critical,
                x if x > self.minimum_warning => State::Warning,
                x if x > self.minimum_info => State::Info,
                _ => State::Good,
            }
        });

        Ok(Some(self.update_interval))
    }

    fn view(&self) -> Vec<&I3BarWidget> {
        vec![&self.output]
    }

    fn click(&mut self, e: &I3BarEvent) -> Result<()> {
        if let Some(ref name) = e.name {
            if name.as_str() == self.id {
                match e.button {
                    MouseButton::Left => self.collapsed = !self.collapsed,
                    MouseButton::Right => self.is_idle = !self.is_idle,
                    MouseButton::WheelUp => self.actual_line += 1,
                    MouseButton::WheelDown => {
                        self.actual_line = if self.actual_line == 0 { usize::max_value() } else { self.actual_line - 1 };
                    },
                    _ => {}
                }

                self.tx_update_request.send(Task {
                    id: self.id.clone(),
                    update_time: Instant::now(),
                });
            }
        }

        Ok(())
    }

    fn id(&self) -> &str {
        &self.id
    }
}


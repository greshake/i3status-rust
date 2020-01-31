use crossbeam_channel::Sender;
use serde_derive::Deserialize;
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::blocks::{Block, ConfigBlock};
use crate::config::Config;
use crate::errors::*;
use crate::input::{I3BarEvent, MouseButton};
use crate::scheduler::Task;
use crate::widget::I3BarWidget;
use crate::widgets::button::ButtonWidget;

use uuid::Uuid;

enum State {
    Started,
    Stopped,
    Paused,
    OnBreak,
}

pub struct Pomodoro {
    id: String,
    time: ButtonWidget,
    state: State,
    elapsed: usize,
    length: usize,
    break_length: usize,
    update_interval: Duration,
    message: String,
    break_message: String,
    count: usize,
    enable_i3nagbar: bool,
}

impl Pomodoro {
    fn set_text(&mut self) {
        self.time
            .set_text(format!("{} | {}", self.count, self.get_text()));
    }

    fn get_text(&self) -> String {
        match self.state {
            State::Stopped => "\u{25a0} 0:00".to_string(),
            State::Started => format!("\u{f04b} {}:{:02}", self.elapsed / 60, self.elapsed % 60),
            State::Paused => format!("\u{f04c} {}:{:02}", self.elapsed / 60, self.elapsed % 60),
            State::OnBreak => format!("\u{2615} {}:{:02}", self.elapsed / 60, self.elapsed % 60),
        }
    }

    fn tick(&mut self) {
        match &self.state {
            State::Stopped => {}
            State::Started => {
                self.elapsed += 1;
            }
            State::Paused => {}
            State::OnBreak => {
                self.elapsed += 1;
            }
        };
    }
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct PomodoroConfig {
    #[serde(default = "PomodoroConfig::default_length")]
    pub length: usize,
    #[serde(default = "PomodoroConfig::default_break_length")]
    pub break_length: usize,
    #[serde(default = "PomodoroConfig::default_message")]
    pub message: String,
    #[serde(default = "PomodoroConfig::default_break_message")]
    pub break_message: String,
    #[serde(default = "PomodoroConfig::default_use_nag")]
    pub use_nag: bool,
}

impl PomodoroConfig {
    fn default_length() -> usize {
        25
    }

    fn default_break_length() -> usize {
        5
    }

    fn default_message() -> String {
        "Pomodoro over! Take a break!".to_owned()
    }

    fn default_break_message() -> String {
        "Break over! Time to work!".to_owned()
    }

    fn default_use_nag() -> bool {
        false
    }
}

impl ConfigBlock for Pomodoro {
    type Config = PomodoroConfig;

    fn new(block_config: Self::Config, config: Config, _send: Sender<Task>) -> Result<Self> {
        let id: String = Uuid::new_v4().to_simple().to_string();

        Ok(Pomodoro {
            id: id.clone(),
            time: ButtonWidget::new(config, &id).with_icon("pomodoro"),
            state: State::Stopped,
            length: block_config.length * 60, // convert to minutes
            break_length: block_config.break_length * 60, // convert to minutes
            update_interval: Duration::from_millis(1000),
            message: block_config.message,
            break_message: block_config.break_message,
            enable_i3nagbar: block_config.use_nag,
            elapsed: 0,
            count: 0,
        })
    }
}

impl Block for Pomodoro {
    fn id(&self) -> &str {
        &self.id
    }

    fn update(&mut self) -> Result<Option<Duration>> {
        self.tick();
        self.set_text();

        match &self.state {
            State::Started => {
                if self.elapsed >= self.length {
                    if self.enable_i3nagbar {
                        nag(&self.message, "error");
                    }

                    self.state = State::OnBreak;
                    self.elapsed = 0;
                    self.count = self.count + 1;
                }
            }
            State::OnBreak => {
                if self.elapsed >= self.break_length {
                    if self.enable_i3nagbar {
                        nag(&self.break_message, "warning");
                    }
                    self.state = State::Stopped;
                }
            }
            _ => {}
        }

        Ok(Some(self.update_interval))
    }

    fn click(&mut self, event: &I3BarEvent) -> Result<()> {
        if let Some(ref name) = event.name {
            if name.as_str() == self.id {
                match event.button {
                    MouseButton::Right => {
                        self.state = State::Stopped;
                        self.elapsed = 0;
                        self.count = 0;
                    }
                    _ => match &self.state {
                        State::Stopped => {
                            self.state = State::Started;
                            self.elapsed = 0;
                        }
                        State::Started => {
                            self.state = State::Paused;
                        }
                        State::Paused => {
                            self.state = State::Started;
                        }
                        State::OnBreak => {
                            self.state = State::Started;
                            self.elapsed = 0;
                        }
                    },
                }
            }
        }

        self.set_text();
        Ok(())
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.time]
    }
}

fn nag(message: &str, level: &str) {
    Command::new("i3-nagbar")
        .stdout(Stdio::null())
        .args(&["-t", level, "-m", message])
        .spawn()
        .expect("Failed to start i3-nagbar");
}

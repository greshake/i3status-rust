use std::fmt;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::SharedConfig;
use crate::errors::*;
use crate::protocol::i3bar_event::{I3BarEvent, MouseButton};
use crate::scheduler::Task;
use crate::subprocess::spawn_child_async;
use crate::widgets::text::TextWidget;
use crate::widgets::I3BarWidget;

enum State {
    Started(Instant),
    Stopped,
    Paused(Duration),
    OnBreak(Instant),
}

impl State {
    fn elapsed(&self) -> Duration {
        match self {
            State::Started(start) => Instant::now().duration_since(start.to_owned()),
            State::Stopped => unreachable!(),
            State::Paused(duration) => duration.to_owned(),
            State::OnBreak(start) => Instant::now().duration_since(start.to_owned()),
        }
    }
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            State::Stopped => write!(f, "0:00"),
            State::Started(_) | State::OnBreak(_) => write!(
                f,
                "{}:{:02}",
                self.elapsed().as_secs() / 60,
                self.elapsed().as_secs() % 60
            ),
            State::Paused(duration) => write!(
                f,
                "{}:{:02}",
                duration.as_secs() / 60,
                duration.as_secs() % 60
            ),
        }
    }
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Notifier {
    I3Nag,
    SwayNag,
    NotifySend,
    None,
}

pub struct Pomodoro {
    id: usize,
    time: TextWidget,
    state: State,
    length: Duration,
    break_length: Duration,
    update_interval: Duration,
    message: String,
    break_message: String,
    count: usize,
    notifier: Notifier,
    notifier_path: std::path::PathBuf,
    shared_config: SharedConfig,
    // Following two are deprecated - remove in a later release
    use_nag: bool,
    nag_path: std::path::PathBuf,
}

impl Pomodoro {
    fn set_text(&mut self) {
        let state_icon = match &self.state {
            State::Stopped => "pomodoro_stopped".to_string(),
            State::Started(_) => "pomodoro_started".to_string(),
            State::OnBreak(_) => "pomodoro_break".to_string(),
            State::Paused(_) => "pomodoro_paused".to_string(),
        };

        self.time.set_text(format!(
            "{} | {} {}",
            self.count,
            self.shared_config.get_icon(&state_icon).unwrap(),
            self.state
        ));
    }

    fn notify(&self, message: &str, level: String) {
        let urgency = if level == "error" {
            "critical".to_string()
        } else {
            "normal".to_string()
        };
        let args = if self.notifier == Notifier::NotifySend {
            ["--urgency", &urgency, message, " "]
        } else {
            ["--type", &level, "--message", message]
        };

        let binary = if self.use_nag {
            self.nag_path.to_str().unwrap()
        } else {
            self.notifier_path.to_str().unwrap()
        };

        spawn_child_async(binary, &args).expect("Failed to start notifier");
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct PomodoroConfig {
    pub length: u64,
    pub break_length: u64,
    pub message: String,
    pub break_message: String,
    pub notifier: Notifier,
    pub notifier_path: Option<std::path::PathBuf>,
    // Following two are deprecated - remove in a later release
    pub use_nag: bool,
    pub nag_path: std::path::PathBuf,
}

impl Default for PomodoroConfig {
    fn default() -> Self {
        Self {
            length: 25,
            break_length: 5,
            message: "Pomodoro over! Take a break!".to_string(),
            break_message: "Break over! Time to work!".to_string(),
            notifier: Notifier::None,
            notifier_path: None,
            // Following two are deprecated - remove in a later release
            use_nag: false,
            nag_path: std::path::PathBuf::from("i3-nagbar"),
        }
    }
}

impl ConfigBlock for Pomodoro {
    type Config = PomodoroConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        shared_config: SharedConfig,
        _send: Sender<Task>,
    ) -> Result<Self> {
        Ok(Pomodoro {
            id,
            time: TextWidget::new(id, 0, shared_config.clone()).with_icon("pomodoro")?,
            state: State::Stopped,
            length: Duration::from_secs(block_config.length * 60), // convert to minutes
            break_length: Duration::from_secs(block_config.break_length * 60), // convert to minutes
            update_interval: Duration::from_millis(1000),
            message: block_config.message,
            break_message: block_config.break_message,
            count: 0,
            notifier: block_config.notifier.clone(),
            notifier_path: if let Some(p) = block_config.notifier_path {
                p
            } else {
                match block_config.notifier {
                    Notifier::I3Nag => std::path::PathBuf::from("i3-nagbar"),
                    Notifier::SwayNag => std::path::PathBuf::from("swaynag"),
                    Notifier::NotifySend => std::path::PathBuf::from("notify-send"),
                    _ => std::path::PathBuf::from(""),
                }
            },
            shared_config,
            // Following two are deprecated - remove in a later release
            use_nag: block_config.use_nag,
            nag_path: block_config.nag_path,
        })
    }
}

impl Block for Pomodoro {
    fn id(&self) -> usize {
        self.id
    }

    fn update(&mut self) -> Result<Option<Update>> {
        self.set_text();
        match &self.state {
            State::Started(_) => {
                if self.state.elapsed() >= self.length {
                    if self.use_nag || self.notifier != Notifier::None {
                        self.notify(&self.message, "error".to_string());
                    }

                    self.state = State::OnBreak(Instant::now());
                }
            }
            State::OnBreak(_) => {
                if self.state.elapsed() >= self.break_length {
                    if self.use_nag || self.notifier != Notifier::None {
                        self.notify(&self.break_message, "warning".to_string());
                    }
                    self.state = State::Stopped;
                    self.count += 1;
                }
            }
            _ => {}
        }

        Ok(Some(self.update_interval.into()))
    }

    fn click(&mut self, event: &I3BarEvent) -> Result<()> {
        match event.button {
            MouseButton::Right => {
                self.state = State::Stopped;
                self.count = 0;
            }
            _ => match &self.state {
                State::Stopped => {
                    self.state = State::Started(Instant::now());
                }
                State::Started(_) => {
                    self.state = State::Paused(self.state.elapsed());
                }
                State::Paused(duration) => {
                    self.state =
                        State::Started(Instant::now().checked_sub(duration.to_owned()).unwrap());
                }
                State::OnBreak(_) => {
                    self.state = State::Started(Instant::now());
                }
            },
        }
        self.set_text();

        Ok(())
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.time]
    }
}

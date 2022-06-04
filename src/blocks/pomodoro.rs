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

#[derive(Clone, Copy)]
enum State {
    Started(Instant),
    Stopped,
    Paused(Duration),
    OnBreak(Instant),
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stopped => write!(f, "0:00"),
            Self::Started(i) | State::OnBreak(i) => {
                let elapsed = i.elapsed();
                write!(
                    f,
                    "{}:{:02}",
                    elapsed.as_secs() / 60,
                    elapsed.as_secs() % 60
                )
            }
            Self::Paused(duration) => write!(
                f,
                "{}:{:02}",
                duration.as_secs() / 60,
                duration.as_secs() % 60
            ),
        }
    }
}

#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Notifier {
    I3Nag,
    SwayNag,
    NotifySend,
    None,
}

pub struct Pomodoro {
    time: TextWidget,
    state: State,
    length: Duration,
    break_length: Duration,
    update_interval: Duration,
    message: String,
    break_message: String,
    count: usize,
    notifier: Notifier,
    notifier_path: String,
    shared_config: SharedConfig,
    // Following two are deprecated - remove in a later release
    use_nag: bool,
    nag_path: String,
}

impl Pomodoro {
    fn set_text(&mut self) -> Result<()> {
        let state_icon = match self.state {
            State::Stopped => "pomodoro_stopped",
            State::Started(_) => "pomodoro_started",
            State::OnBreak(_) => "pomodoro_break",
            State::Paused(_) => "pomodoro_paused",
        };

        self.time.set_text(format!(
            "{} | {} {}",
            self.count,
            self.shared_config.get_icon(state_icon)?,
            self.state
        ));

        Ok(())
    }

    fn notify(&self, message: &str, level: &str) -> Result<()> {
        let urgency = if level == "error" {
            "critical"
        } else {
            "normal"
        };
        let args = if self.notifier == Notifier::NotifySend {
            ["--urgency", urgency, message, " "]
        } else {
            ["--type", level, "--message", message]
        };

        let binary = if self.use_nag {
            &self.nag_path
        } else {
            &self.notifier_path
        };

        spawn_child_async(binary, &args).error_msg("Failed to start notifier")?;
        Ok(())
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
    pub notifier_path: Option<String>,
    // Following two are deprecated - remove in a later release
    pub use_nag: bool,
    pub nag_path: String,
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
            nag_path: "i3-nagbar".to_string(),
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
                    Notifier::I3Nag => "i3-nagbar",
                    Notifier::SwayNag => "swaynag",
                    Notifier::NotifySend => "notify-send",
                    _ => "",
                }
                .into()
            },
            shared_config,
            // Following two are deprecated - remove in a later release
            use_nag: block_config.use_nag,
            nag_path: block_config.nag_path,
        })
    }
}

impl Block for Pomodoro {
    fn name(&self) -> &'static str {
        "pomodoro"
    }

    fn update(&mut self) -> Result<Option<Update>> {
        self.set_text()?;
        match self.state {
            State::Started(started) => {
                if started.elapsed() >= self.length {
                    if self.use_nag || self.notifier != Notifier::None {
                        self.notify(&self.message, "error")?;
                    }

                    self.state = State::OnBreak(Instant::now());
                }
            }
            State::OnBreak(on_break) => {
                if on_break.elapsed() >= self.break_length {
                    if self.use_nag || self.notifier != Notifier::None {
                        self.notify(&self.break_message, "warning")?;
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
            _ => {
                self.state = match self.state {
                    State::Stopped | State::OnBreak(_) => State::Started(Instant::now()),
                    State::Started(started) => State::Paused(started.elapsed()),
                    State::Paused(duration) => State::Started(Instant::now() - duration),
                };
            }
        }
        self.set_text()
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.time]
    }
}

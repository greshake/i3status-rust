use std::collections::BTreeMap;
use std::fmt;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::Config;
use crate::errors::*;
use crate::input::{I3BarEvent, MouseButton};
use crate::scheduler::Task;
use crate::subprocess::spawn_child_async;
use crate::widget::I3BarWidget;
use crate::widgets::button::ButtonWidget;

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
            State::Stopped => write!(f, "\u{25a0} 0:00"),
            State::Started(_) => write!(
                f,
                "\u{f04b} {}:{:02}",
                self.elapsed().as_secs() / 60,
                self.elapsed().as_secs() % 60
            ),
            State::OnBreak(_) => write!(
                f,
                "\u{2615} {}:{:02}",
                self.elapsed().as_secs() / 60,
                self.elapsed().as_secs() % 60
            ),
            State::Paused(duration) => write!(
                f,
                "\u{f04c} {}:{:02}",
                duration.as_secs() / 60,
                duration.as_secs() % 60
            ),
        }
    }
}

pub struct Pomodoro {
    id: usize,
    time: ButtonWidget,
    state: State,
    length: Duration,
    break_length: Duration,
    update_interval: Duration,
    message: String,
    break_message: String,
    count: usize,
    use_nag: bool,
    nag_path: std::path::PathBuf,
}

impl Pomodoro {
    fn set_text(&mut self) {
        self.time
            .set_text(format!("{} | {}", self.count, self.state));
    }

    fn nag(&self, message: &str, level: &str) {
        spawn_child_async(
            self.nag_path.to_str().unwrap(),
            &["-t", level, "-m", message],
        )
        .expect("Failed to start i3-nagbar");
    }
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct PomodoroConfig {
    #[serde(default = "PomodoroConfig::default_length")]
    pub length: u64,
    #[serde(default = "PomodoroConfig::default_break_length")]
    pub break_length: u64,
    #[serde(default = "PomodoroConfig::default_message")]
    pub message: String,
    #[serde(default = "PomodoroConfig::default_break_message")]
    pub break_message: String,
    #[serde(default = "PomodoroConfig::default_use_nag")]
    pub use_nag: bool,
    #[serde(default = "PomodoroConfig::default_nag_path")]
    pub nag_path: std::path::PathBuf,
    #[serde(default = "PomodoroConfig::default_color_overrides")]
    pub color_overrides: Option<BTreeMap<String, String>>,
}

impl PomodoroConfig {
    fn default_length() -> u64 {
        25
    }

    fn default_break_length() -> u64 {
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

    fn default_nag_path() -> std::path::PathBuf {
        std::path::PathBuf::from("i3-nagbar")
    }

    fn default_color_overrides() -> Option<BTreeMap<String, String>> {
        None
    }
}

impl ConfigBlock for Pomodoro {
    type Config = PomodoroConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        config: Config,
        _send: Sender<Task>,
    ) -> Result<Self> {
        Ok(Pomodoro {
            id,
            time: ButtonWidget::new(config, id).with_icon("pomodoro"),
            state: State::Stopped,
            length: Duration::from_secs(block_config.length * 60), // convert to minutes
            break_length: Duration::from_secs(block_config.break_length * 60), // convert to minutes
            update_interval: Duration::from_millis(1000),
            message: block_config.message,
            break_message: block_config.break_message,
            use_nag: block_config.use_nag,
            count: 0,
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
                    if self.use_nag {
                        self.nag(&self.message, "error");
                    }

                    self.state = State::OnBreak(Instant::now());
                }
            }
            State::OnBreak(_) => {
                if self.state.elapsed() >= self.break_length {
                    if self.use_nag {
                        self.nag(&self.break_message, "warning");
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
        if event.matches_id(self.id) {
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
                        self.state = State::Started(
                            Instant::now().checked_sub(duration.to_owned()).unwrap(),
                        );
                    }
                    State::OnBreak(_) => {
                        self.state = State::Started(Instant::now());
                    }
                },
            }
            self.set_text();
        }

        Ok(())
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.time]
    }
}

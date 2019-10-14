use crossbeam_channel::Sender;
use std::process::Command;
use std::time::Duration;

use crate::blocks::{Block, ConfigBlock};
use crate::config::Config;
use crate::errors::*;
use crate::input::{I3BarEvent, MouseButton};
use crate::scheduler::Task;
use crate::widget::I3BarWidget;
use crate::widgets::text::TextWidget;

use uuid::Uuid;

machine!(
    #[derive(Clone, Debug, PartialEq)]
    enum State {
        Stopped { text: String },
        Started { text: String, seconds: usize },
        Paused { text: String, seconds: usize },
    }
);

#[derive(Clone, Debug, PartialEq)]
pub struct Start;
#[derive(Clone, Debug, PartialEq)]
pub struct Pause;
#[derive(Clone, Debug, PartialEq)]
pub struct Stop;

/*
 * can't use macro because we're borrowing as mut
transitions!(State,
    [
        (Stopped, Start) => Started,
        (Started, Pause) => Paused,
        (Started, Stop) => Stopped,
        (Paused, Start) => Started,
        (Paused, Stop) => Stopped
    ]
);
*/

#[derive(Clone, Debug, PartialEq)]
pub enum StateMessages {
    Start(Start),
    Stop(Stop),
    Pause(Pause),
}

impl State {
    pub fn on_start(&mut self, input: Start) -> State {
        match self {
            State::Stopped(state) => State::Started(state.on_start(input)),
            State::Paused(state) => State::Started(state.on_start(input)),
            _ => State::Error,
        }
    }

    pub fn on_stop(&mut self, input: Stop) -> State {
        match self {
            State::Started(state) => State::Stopped(state.on_stop(input)),
            State::Paused(state) => State::Stopped(state.on_stop(input)),
            _ => State::Error,
        }
    }

    pub fn on_pause(&mut self, input: Pause) -> State {
        match self {
            State::Started(state) => State::Paused(state.on_pause(input)),
            _ => State::Error,
        }
    }
}

impl Stopped {
    pub fn on_start(&mut self, _: Start) -> Started {
        Started {
            seconds: 0,
            text: "started".to_string(),
        }
    }
}

impl Started {
    pub fn on_pause(&mut self, _: Pause) -> Paused {
        Paused {
            seconds: self.seconds,
            text: "paused".to_string(),
        }
    }

    pub fn on_stop(&mut self, _: Stop) -> Stopped {
        Stopped { text: "stopped".to_string() }
    }
}

impl Paused {
    pub fn on_start(&mut self, _: Start) -> Started {
        Started {
            seconds: self.seconds,
            text: "started".to_string(),
        }
    }

    pub fn on_stop(&mut self, _: Stop) -> Stopped {
        Stopped { text: "stopped".to_string() }
    }
}

methods!(State,
  [
    Stopped, Started, Paused => fn get_text(&self) -> String,
    Started => get seconds: usize
  ]
);

impl Stopped {
    pub fn get_text(&self) -> String {
        self.text.to_owned()
    }
}

impl State {
    pub fn tick(&mut self) -> Option<()> {
        match self {
            State::Started(ref mut v) => Some(v.tick()),
            _ => None,
        }
    }
}

impl Started {
    pub fn get_text(&self) -> String {
        format!("{} {}", self.text, self.seconds)
    }

    pub fn tick(&mut self) -> () {
        self.seconds = self.seconds + 1;
    }
}

impl Paused {
    pub fn get_text(&self) -> String {
        format!("{} {}", self.text, self.seconds)
    }
}

pub struct Pomodoro {
    id: String,
    time: TextWidget,
    state: State,
    pomodoro_length: usize,
    update_interval: Duration,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct PomodoroConfig {
    #[serde(default = "PomodoroConfig::default_pomodoro_length")]
    pub pomodoro_length: usize,
}

impl PomodoroConfig {
    fn default_pomodoro_length() -> usize {
        25
    }
}

impl ConfigBlock for Pomodoro {
    type Config = PomodoroConfig;

    fn new(block_config: Self::Config, config: Config, _send: Sender<Task>) -> Result<Self> {
        let id: String = Uuid::new_v4().simple().to_string();
        let id_copy = id.clone();

        Ok(Pomodoro {
            id: id_copy,
            time: TextWidget::new(config),
            state: State::stopped("stopped".to_string()),
            pomodoro_length: block_config.pomodoro_length,
            update_interval: Duration::from_millis(1000),
        })
    }
}

impl Block for Pomodoro {
    fn id(&self) -> &str {
        &self.id
    }

    fn update(&mut self) -> Result<Option<Duration>> {
        self.state.tick();

        self.time.set_text(format!("{}", self.state.get_text().unwrap()));

        if let Some(seconds) = self.state.seconds() {
            // TODO add * 60 to converto to minutes
            if seconds > &self.pomodoro_length {
                std::thread::spawn(|| -> Result<()> {
                    match Command::new("i3-nagbar").args(&["-m", "Pomodoro over"]).output() {
                        Ok(_raw_output) => Ok(()),
                        Err(_) => {
                            // We don't want the bar to crash if i3-nagbar fails
                            Ok(())
                        }
                    }
                });

                self.state = self.state.on_stop(Stop);
            }
        }

        Ok(Some(self.update_interval))
    }

    fn click(&mut self, event: &I3BarEvent) -> Result<()> {
        match event.button {
            MouseButton::Right => match &self.state {
                State::Started(_state) => {
                    self.state = self.state.on_stop(Stop);
                }
                _ => {}
            },
            _ => match &self.state {
                State::Stopped(_state) => {
                    self.state = self.state.on_start(Start);
                }
                State::Started(_state) => {
                    self.state = self.state.on_pause(Pause);
                }
                State::Paused(_state) => {
                    self.state = self.state.on_start(Start);
                }
                _ => {}
            },
        }

        self.time.set_text(self.state.get_text().unwrap());
        Ok(())
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.time]
    }
}

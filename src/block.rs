use std::time::Duration;
use serde_json::Value;

#[derive(Debug)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Copy, Clone)]
pub enum State {
    Idle,
    Info,
    Good,
    Warning,
    Critical
}

impl State {
    pub fn theme_keys(self) -> (&'static str, &'static str) {
        use self::State::*;
        match self {
            Idle => ("idle_bg", "idle_fg"),
            Info => ("info_bg", "info_fg"),
            Good => ("good_bg", "good_fg"),
            Warning => ("warning_bg", "warning_fg"),
            Critical => ("critical_bg", "critical_fg"),
        }
    }
}

pub trait Block {
    fn get_status(&self, theme: &Value) -> Value;
    fn get_state(&self) -> State { State::Idle }
    fn update(&self) -> Option<Duration> {
        None
    }

    fn id(&self) -> Option<&str> {
        None
    }
    fn click(&self, MouseButton) {}
}

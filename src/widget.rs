use serde_json::Value;

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

pub trait I3BarWidget {
    fn to_string(&self) -> String;
    fn get_rendered(&self) -> &Value;
}
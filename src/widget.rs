use crate::themes::Theme;
use serde_json::value::Value;
use std::convert::TryFrom;

#[derive(Debug, Copy, Clone)]
pub enum State {
    Idle,
    Info,
    Good,
    Warning,
    Critical,
}

impl State {
    pub fn theme_keys(self, theme: &Theme) -> (&String, &String) {
        use self::State::*;
        match self {
            Idle => (&theme.idle_bg, &theme.idle_fg),
            Info => (&theme.info_bg, &theme.info_fg),
            Good => (&theme.good_bg, &theme.good_fg),
            Warning => (&theme.warning_bg, &theme.warning_fg),
            Critical => (&theme.critical_bg, &theme.critical_fg),
        }
    }
}

impl TryFrom<String> for State {
    type Error = &'static str;

    fn try_from(state: String) -> Result<Self, Self::Error> {
        match state.to_lowercase().as_ref() {
            "idle" => Ok(Self::Idle),
            "info" => Ok(Self::Info),
            "good" => Ok(Self::Good),
            "warning" => Ok(Self::Warning),
            "critical" => Ok(Self::Critical),
            _ => Err("Not a valid state.")
        }
    }
}

pub trait I3BarWidget {
    fn to_string(&self) -> String;
    fn get_rendered(&self) -> &Value;
}

use themes::Theme;
use serde_json::value::Value;

#[derive(Debug, Copy, Clone)]
pub enum State {
    Idle,
    Info,
    Good,
    Warning,
    Critical
}

impl State {
    pub fn theme_keys(self, theme: &Theme) -> (&String, &String) {
        use self::State::*;
        match self {
            Idle => (&theme.idle_bg, &theme.idle_fg), //("idle_bg", "idle_fg"),
            Info => (&theme.info_bg, &theme.info_fg), //("info_bg", "info_fg"),
            Good => (&theme.good_bg, &theme.good_fg), //("good_bg", "good_fg"),
            Warning => (&theme.warning_bg, &theme.warning_fg), //("warning_bg", "warning_fg"),
            Critical => (&theme.critical_bg, &theme.critical_fg), //("critical_bg", "critical_fg"),
        }
    }
}

pub trait I3BarWidget {
    fn to_string(&self) -> String;
    fn get_rendered(&self) -> &Value;
}

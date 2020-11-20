use std::str::FromStr;

use serde::de::value::{Error, StrDeserializer};
use serde::de::{Deserialize, IntoDeserializer};
use serde_derive::Deserialize;
use serde_json::value::Value;

use crate::themes::Theme;

#[derive(Debug, Copy, Clone, Deserialize)]
pub enum Spacing {
    /// Add a leading and trailing space around the widget contents
    Normal,
    /// Hide the leading space when the widget is inline
    Inline,
    /// Hide both leading and trailing spaces when widget is hidden
    Hidden,
}

#[derive(Debug, Copy, Clone, Deserialize)]
pub enum State {
    Idle,
    Info,
    Good,
    Warning,
    Critical,
}

impl State {
    pub fn theme_keys(self, theme: &Theme) -> (&Option<String>, &Option<String>) {
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

impl FromStr for State {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let deserializer: StrDeserializer<Error> = s.into_deserializer();
        State::deserialize(deserializer).map_err(|_| ())
    }
}

pub trait I3BarWidget {
    fn to_string(&self) -> String;
    fn get_rendered(&self) -> &Value;
}

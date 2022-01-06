pub mod rotatingtext;
pub mod text;

use std::str::FromStr;

use serde::de::value::{Error, StrDeserializer};
use serde::de::{Deserialize, IntoDeserializer};
use serde_derive::Deserialize;

use crate::protocol::i3bar_block::I3BarBlock;
use crate::themes::{Color, Theme};

#[derive(Debug, Copy, Clone, Deserialize)]
pub enum Spacing {
    /// Add a leading and trailing space around the widget contents
    Normal,
    /// Hide the leading space when the widget is inline
    Inline,
    /// Hide both leading and trailing spaces when widget is hidden
    Hidden,
}

impl Spacing {
    pub fn from_content(content: &str) -> Self {
        if content.is_empty() {
            Self::Hidden
        } else {
            Self::Normal
        }
    }

    pub fn to_string_leading(self) -> String {
        match self {
            Self::Normal => String::from(" "),
            _ => String::from(""),
        }
    }

    pub fn to_string_trailing(self) -> String {
        match self {
            Self::Hidden => String::from(""),
            _ => String::from(" "),
        }
    }
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
    pub fn theme_keys(self, theme: &Theme) -> (Color, Color) {
        use self::State::*;
        match self {
            Idle => (theme.idle_bg, theme.idle_fg),
            Info => (theme.info_bg, theme.info_fg),
            Good => (theme.good_bg, theme.good_fg),
            Warning => (theme.warning_bg, theme.warning_fg),
            Critical => (theme.critical_bg, theme.critical_fg),
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
    fn get_data(&self) -> I3BarBlock;
}

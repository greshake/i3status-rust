pub mod color;

use serde::Deserialize;
use std::collections::HashMap;

use crate::errors::*;
use crate::util;
use crate::widget::State;
use color::Color;

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(try_from = "ThemeConfigRaw")]
pub struct Theme {
    pub idle_bg: Color,
    pub idle_fg: Color,
    pub info_bg: Color,
    pub info_fg: Color,
    pub good_bg: Color,
    pub good_fg: Color,
    pub warning_bg: Color,
    pub warning_fg: Color,
    pub critical_bg: Color,
    pub critical_fg: Color,
    pub separator: Option<String>,
    pub separator_bg: Color,
    pub separator_fg: Color,
    pub alternating_tint_bg: Color,
    pub alternating_tint_fg: Color,
}

impl Theme {
    pub fn from_file(file: &str) -> Result<Theme> {
        let file = util::find_file(file, Some("themes"), Some("toml"))
            .or_error(|| format!("Theme '{}' not found", file))?;
        let map: HashMap<String, String> = util::deserialize_toml_file(&file)?;
        let mut theme = Self::default();
        theme.apply_overrides(&map)?;
        Ok(theme)
    }

    pub fn get_colors(&self, state: State) -> (Color, Color) {
        match state {
            State::Idle => (self.idle_bg, self.idle_fg),
            State::Info => (self.info_bg, self.info_fg),
            State::Good => (self.good_bg, self.good_fg),
            State::Warning => (self.warning_bg, self.warning_fg),
            State::Critical => (self.critical_bg, self.critical_fg),
        }
    }

    pub fn apply_overrides(&mut self, overrides: &HashMap<String, String>) -> Result<()> {
        if let Some(separator) = overrides.get("separator") {
            if separator == "native" {
                self.separator = None;
            } else {
                self.separator = Some(separator.clone());
            }
        }
        macro_rules! apply {
            ($prop:tt) => {
                if let Some(val) = overrides.get(stringify!($prop)) {
                    self.$prop = val.parse()?;
                }
            };
        }
        apply!(idle_bg);
        apply!(idle_fg);
        apply!(info_bg);
        apply!(info_fg);
        apply!(good_bg);
        apply!(good_fg);
        apply!(warning_bg);
        apply!(warning_fg);
        apply!(critical_bg);
        apply!(critical_fg);
        apply!(separator_bg);
        apply!(separator_fg);
        apply!(alternating_tint_bg);
        apply!(alternating_tint_fg);
        Ok(())
    }
}

#[derive(Deserialize)]
struct ThemeConfigRaw {
    theme: Option<String>,
    overrides: Option<HashMap<String, String>>,
}

impl TryFrom<ThemeConfigRaw> for Theme {
    type Error = Error;

    fn try_from(raw: ThemeConfigRaw) -> Result<Self, Self::Error> {
        let mut theme = Self::from_file(raw.theme.as_deref().unwrap_or("plain"))?;
        if let Some(overrides) = &raw.overrides {
            theme.apply_overrides(overrides)?;
        }
        Ok(theme)
    }
}

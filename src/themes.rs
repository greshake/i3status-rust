pub mod color;
pub mod separator;

use std::fmt;
use std::ops::{Deref, DerefMut};

use serde::{de, Deserialize};

use crate::errors::*;
use crate::util;
use crate::widget::State;
use color::Color;
use separator::Separator;

#[derive(Debug, Clone)]
pub struct Theme(pub ThemeInner);

impl Default for Theme {
    fn default() -> Self {
        ThemeUserConfig::default()
            .try_into()
            .unwrap_or_else(|_| Self(ThemeInner::default()))
    }
}

impl Deref for Theme {
    type Target = ThemeInner;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for Theme {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields, default)]
pub struct ThemeInner {
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
    pub separator: Separator,
    pub separator_bg: Color,
    pub separator_fg: Color,
    pub alternating_tint_bg: Color,
    pub alternating_tint_fg: Color,
    pub end_separator: Separator,
}

impl Theme {
    pub fn get_colors(&self, state: State) -> (Color, Color) {
        match state {
            State::Idle => (self.idle_bg, self.idle_fg),
            State::Info => (self.info_bg, self.info_fg),
            State::Good => (self.good_bg, self.good_fg),
            State::Warning => (self.warning_bg, self.warning_fg),
            State::Critical => (self.critical_bg, self.critical_fg),
        }
    }

    pub fn apply_overrides(&mut self, overrides: ThemeOverrides) -> Result<()> {
        let copy = self.clone();

        if let Some(separator) = overrides.separator {
            self.separator = separator;
        }
        if let Some(end_separator) = overrides.end_separator {
            self.end_separator = end_separator;
        }

        macro_rules! apply {
            ($prop:tt) => {
                if let Some(color) = overrides.$prop {
                    self.$prop = color.eval(&copy)?;
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

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, default)]
pub struct ThemeUserConfig {
    pub theme: Option<String>,
    pub overrides: Option<ThemeOverrides>,
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct ThemeOverrides {
    pub idle_bg: Option<ColorOrLink>,
    pub idle_fg: Option<ColorOrLink>,
    pub info_bg: Option<ColorOrLink>,
    pub info_fg: Option<ColorOrLink>,
    pub good_bg: Option<ColorOrLink>,
    pub good_fg: Option<ColorOrLink>,
    pub warning_bg: Option<ColorOrLink>,
    pub warning_fg: Option<ColorOrLink>,
    pub critical_bg: Option<ColorOrLink>,
    pub critical_fg: Option<ColorOrLink>,
    pub separator: Option<Separator>,
    pub separator_bg: Option<ColorOrLink>,
    pub separator_fg: Option<ColorOrLink>,
    pub alternating_tint_bg: Option<ColorOrLink>,
    pub alternating_tint_fg: Option<ColorOrLink>,
    pub end_separator: Option<Separator>,
}

impl TryFrom<ThemeUserConfig> for Theme {
    type Error = Error;

    fn try_from(user_config: ThemeUserConfig) -> Result<Self, Self::Error> {
        let name = user_config.theme.as_deref().unwrap_or("plain");
        let file = util::find_file(name, Some("themes"), Some("toml"))
            .or_error(|| format!("Theme '{name}' not found"))?;
        let theme: ThemeInner = util::deserialize_toml_file(file)?;
        let mut theme = Theme(theme);
        if let Some(overrides) = user_config.overrides {
            theme.apply_overrides(overrides)?;
        }
        Ok(theme)
    }
}

#[derive(Debug, Clone)]
pub enum ColorOrLink {
    Color(Color),
    Link { link: String },
}

impl ColorOrLink {
    fn eval(self, theme: &Theme) -> Result<Color> {
        Ok(match self {
            Self::Color(c) => c,
            Self::Link { link } => match link.as_str() {
                "idle_bg" => theme.idle_bg,
                "idle_fg" => theme.idle_fg,
                "info_bg" => theme.info_bg,
                "info_fg" => theme.info_fg,
                "good_bg" => theme.good_bg,
                "good_fg" => theme.good_fg,
                "warning_bg" => theme.warning_bg,
                "warning_fg" => theme.warning_fg,
                "critical_bg" => theme.critical_bg,
                "critical_fg" => theme.critical_fg,
                "separator_bg" => theme.separator_bg,
                "separator_fg" => theme.separator_fg,
                "alternating_tint_bg" => theme.alternating_tint_bg,
                "alternating_tint_fg" => theme.alternating_tint_fg,
                _ => return Err(Error::new(format!("{link} is not a correct theme color"))),
            },
        })
    }
}

impl<'de> Deserialize<'de> for ColorOrLink {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        struct Visitor;
        impl<'de> de::Visitor<'de> for Visitor {
            type Value = ColorOrLink;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("color or link")
            }

            fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
            where
                A: de::MapAccess<'de>,
            {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Link {
                    link: String,
                }
                Link::deserialize(de::value::MapAccessDeserializer::new(map))
                    .map(|link| ColorOrLink::Link { link: link.link })
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                v.parse::<Color>().serde_error().map(ColorOrLink::Color)
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}

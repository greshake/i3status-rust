use std::collections::HashMap;
use std::fmt;
use std::ops::Add;
use std::str::FromStr;

use color_space::{Hsv, Rgb};
use serde::de::{self, Deserialize, Deserializer, MapAccess, Visitor};
use serde::{Serialize, Serializer};
use serde_derive::Deserialize;
use smartstring::alias::String;

use crate::errors::{self, OptionExt, ResultExt, ToSerdeError};
use crate::util;
use crate::widget::State;

// TODO docs
// TODO tests
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Color {
    None,
    Auto,
    Rgba(Rgb, u8),
    Hsva(Hsv, u8),
}

impl Serialize for Color {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let format_rgb = |rgb: Rgb, a: u8| {
            format!(
                "#{:02X}{:02X}{:02X}{:02X}",
                rgb.r as u8, rgb.g as u8, rgb.b as u8, a
            )
        };
        match *self {
            Self::None | Self::Auto => serializer.serialize_none(),
            Self::Rgba(rgb, a) => serializer.serialize_str(&format_rgb(rgb, a)),
            Self::Hsva(hsv, a) => serializer.serialize_str(&format_rgb(hsv.into(), a)),
        }
    }
}

impl Color {
    pub fn skip_ser(&self) -> bool {
        matches!(self, Self::None | Self::Auto)
    }
}

impl Default for Color {
    fn default() -> Self {
        Self::None
    }
}

impl Add for Color {
    type Output = Color;
    fn add(self, rhs: Self) -> Self::Output {
        let add_hsv = |a: Hsv, b: Hsv| {
            Hsv::new(
                (a.h + b.h) % 360.,
                (a.s + b.s).clamp(0., 1.),
                (a.v + b.v).clamp(0., 1.),
            )
        };

        match (self, rhs) {
            // Do nothing
            (x, Color::None | Color::Auto) => x,
            (Color::None | Color::Auto, x) => x,
            // Hsv + Hsv => Hsv
            (Color::Hsva(hsv1, a1), Color::Hsva(hsv2, a2)) => {
                Color::Hsva(add_hsv(hsv1, hsv2), a1.saturating_add(a2))
            }
            // Rgb + Rgb => Rgb
            (Color::Rgba(rgb1, a1), Color::Rgba(rgb2, a2)) => Color::Rgba(
                Rgb::new(
                    (rgb1.r + rgb2.r).clamp(0., 255.),
                    (rgb1.g + rgb2.g).clamp(0., 255.),
                    (rgb1.b + rgb2.b).clamp(0., 255.),
                ),
                a1.saturating_add(a2),
            ),
            // Hsv + Rgb => Hsv
            // Rgb + Hsv => Hsv
            (Color::Hsva(hsv, a1), Color::Rgba(rgb, a2))
            | (Color::Rgba(rgb, a1), Color::Hsva(hsv, a2)) => {
                Color::Hsva(add_hsv(hsv, rgb.into()), a1.saturating_add(a2))
            }
        }
    }
}

impl FromStr for Color {
    type Err = crate::errors::Error;
    fn from_str(color: &str) -> Result<Self, Self::Err> {
        Ok(if color == "none" || color.is_empty() {
            Color::None
        } else if color == "auto" {
            Color::Auto
        } else if color.starts_with("hsv:") {
            let err_msg = || format!("'{}' is not a vaild HSVA color", color);
            let color = color.split_at(4).1;
            let mut components = color.split(':').map(|x| x.parse::<f64>().or_error(err_msg));
            let h = components.next().or_error(err_msg)??;
            let s = components.next().or_error(err_msg)??;
            let v = components.next().or_error(err_msg)??;
            let a = components.next().unwrap_or(Ok(100.))?;
            Color::Hsva(Hsv::new(h, s / 100., v / 100.), (a / 100. * 255.) as u8)
        } else {
            let err_msg = || format!("'{}' is not a vaild RGBA color", color);
            let rgb = color.get(1..7).or_error(err_msg)?;
            let a = color.get(7..9).unwrap_or("FF");
            Color::Rgba(
                Rgb::from_hex(u32::from_str_radix(rgb, 16).or_error(err_msg)?),
                u8::from_str_radix(a, 16).or_error(err_msg)?,
            )
        })
    }
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ColorVisitor;

        impl<'de> Visitor<'de> for ColorVisitor {
            type Value = Color;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("color")
            }

            fn visit_str<E>(self, s: &str) -> Result<Color, E>
            where
                E: de::Error,
            {
                s.parse().serde_error()
            }
        }

        deserializer.deserialize_any(ColorVisitor)
    }
}

#[derive(Debug, Clone, Default)]
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
    pub fn from_file(file: &str) -> errors::Result<Theme> {
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

    pub fn apply_overrides(
        &mut self,
        overrides: &HashMap<String, String>,
    ) -> Result<(), crate::errors::Error> {
        if let Some(separator) = overrides.get("separator") {
            self.separator = Some(separator.clone());
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

impl<'de> Deserialize<'de> for Theme {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "lowercase")]
        enum Field {
            Name,
            File,
            Overrides,
        }

        struct ThemeVisitor;

        impl<'de> Visitor<'de> for ThemeVisitor {
            type Value = Theme;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("struct Theme")
            }

            /// Handle configs like:
            ///
            /// ```toml
            /// theme = "slick"
            /// ```
            fn visit_str<E>(self, file: &str) -> Result<Theme, E>
            where
                E: de::Error,
            {
                Theme::from_file(file).serde_error()
            }

            /// Handle configs like:
            ///
            /// ```toml
            /// [theme]
            /// name = "modern"
            /// ```
            fn visit_map<V>(self, mut map: V) -> Result<Theme, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut theme: Option<String> = None;
                let mut overrides: Option<HashMap<String, String>> = None;
                while let Some(key) = map.next_key()? {
                    match key {
                        // TODO merge name and file into one option (let's say "theme")
                        Field::Name => {
                            if theme.is_some() {
                                return Err(de::Error::duplicate_field("name or file"));
                            }
                            theme = Some(map.next_value()?);
                        }
                        Field::File => {
                            if theme.is_some() {
                                return Err(de::Error::duplicate_field("name or file"));
                            }
                            theme = Some(map.next_value()?);
                        }
                        Field::Overrides => {
                            if overrides.is_some() {
                                return Err(de::Error::duplicate_field("overrides"));
                            }
                            overrides = Some(map.next_value()?);
                        }
                    }
                }

                let theme = theme.unwrap_or_else(|| "plain".into());
                let mut theme = Theme::from_file(&theme).serde_error()?;

                if let Some(ref overrides) = overrides {
                    theme.apply_overrides(overrides).serde_error()?;
                }

                Ok(theme)
            }
        }

        deserializer.deserialize_any(ThemeVisitor)
    }
}

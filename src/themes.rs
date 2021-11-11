use std::collections::HashMap;
use std::default::Default;
use std::fmt;
use std::ops::Add;
use std::str::FromStr;

use serde::de::{self, Deserialize, Deserializer, MapAccess, Visitor};
use serde_derive::Deserialize;

use crate::errors::ToSerdeError;
use crate::util;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    None,
    Auto,
    Rgba(u8, u8, u8, u8),
}

impl Add for Color {
    type Output = Color;
    fn add(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (x, Color::None) => x,
            (x, Color::Auto) => x,
            (Color::None, x) => x,
            (Color::Auto, x) => x,
            (Color::Rgba(r1, g1, b1, a1), Color::Rgba(r2, g2, b2, a2)) => Color::Rgba(
                r1.saturating_add(r2),
                g1.saturating_add(g2),
                b1.saturating_add(b2),
                a1.saturating_add(a2),
            ),
        }
    }
}

impl FromStr for Color {
    type Err = crate::errors::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "none" || s.is_empty() {
            Ok(Color::None)
        } else if s == "auto" {
            Ok(Color::Auto)
        } else {
            use crate::errors::{OptionExt, ResultExtInternal};
            let err_msg = "invaild RGBA color";
            let r = s.get(1..3).internal_error("color parser", err_msg)?;
            let g = s.get(3..5).internal_error("color parser", err_msg)?;
            let b = s.get(5..7).internal_error("color parser", err_msg)?;
            let a = s.get(7..9).unwrap_or("FF");
            Ok(Color::Rgba(
                u8::from_str_radix(r, 16).internal_error("color parser", err_msg)?,
                u8::from_str_radix(g, 16).internal_error("color parser", err_msg)?,
                u8::from_str_radix(b, 16).internal_error("color parser", err_msg)?,
                u8::from_str_radix(a, 16).internal_error("color parser", err_msg)?,
            ))
        }
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

impl Color {
    pub fn to_string(self) -> Option<String> {
        match self {
            Color::Rgba(r, g, b, a) => Some(format!("#{:02X}{:02X}{:02X}{:02X}", r, g, b, a)),
            _ => None,
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct InternalTheme {
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

impl Default for InternalTheme {
    fn default() -> Self {
        Self {
            idle_bg: Color::None,
            idle_fg: Color::None,
            info_bg: Color::None,
            info_fg: Color::None,
            good_bg: Color::None,
            good_fg: Color::None,
            warning_bg: Color::None,
            warning_fg: Color::None,
            critical_bg: Color::None,
            critical_fg: Color::None,
            separator: None,
            separator_bg: Color::None,
            separator_fg: Color::None,
            alternating_tint_bg: Color::None,
            alternating_tint_fg: Color::None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Theme(pub InternalTheme);

impl Default for Theme {
    fn default() -> Self {
        Self::from_file("plain").unwrap_or_else(|| Self(InternalTheme::default()))
    }
}

impl std::ops::Deref for Theme {
    type Target = InternalTheme;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for Theme {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Theme {
    pub fn from_file(file: &str) -> Option<Theme> {
        let file = util::find_file(file, Some("themes"), Some("toml"))?;
        Some(Theme(util::deserialize_file(&file).ok()?))
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
                Theme::from_file(file)
                    .ok_or_else(|| de::Error::custom(format!("Theme '{}' not found.", file)))
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

                let theme = theme.unwrap_or_else(|| "plain".to_string());
                let mut theme = Theme::from_file(&theme)
                    .ok_or_else(|| de::Error::custom(format!("Theme '{}' not found.", theme)))?;

                if let Some(ref overrides) = overrides {
                    theme.apply_overrides(overrides).serde_error()?;
                }
                Ok(theme)
            }
        }

        deserializer.deserialize_any(ThemeVisitor)
    }
}

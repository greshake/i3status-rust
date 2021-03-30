use std::default::Default;
use std::fmt;

use serde::de::{self, Deserialize, Deserializer, MapAccess, Visitor};
use serde_derive::Deserialize;

use crate::util;

#[derive(Deserialize, Debug, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct InternalTheme {
    pub idle_bg: Option<String>,
    pub idle_fg: Option<String>,
    pub info_bg: Option<String>,
    pub info_fg: Option<String>,
    pub good_bg: Option<String>,
    pub good_fg: Option<String>,
    pub warning_bg: Option<String>,
    pub warning_fg: Option<String>,
    pub critical_bg: Option<String>,
    pub critical_fg: Option<String>,
    pub separator: Option<String>,
    pub separator_bg: Option<String>,
    pub separator_fg: Option<String>,
    pub alternating_tint_bg: Option<String>,
    pub alternating_tint_fg: Option<String>,
}

impl Default for InternalTheme {
    fn default() -> Self {
        Self {
            idle_bg: None,
            idle_fg: None,
            info_bg: None,
            info_fg: None,
            good_bg: None,
            good_fg: None,
            warning_bg: None,
            warning_fg: None,
            critical_bg: None,
            critical_fg: None,
            separator: None,
            separator_bg: None,
            separator_fg: None,
            alternating_tint_bg: None,
            alternating_tint_fg: None,
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
                let mut theme = None;
                let mut overrides: Option<InternalTheme> = None;
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

                if let Some(overrides) = overrides {
                    theme.0.idle_bg = overrides.idle_bg.or(theme.0.idle_bg);
                    theme.0.idle_fg = overrides.idle_fg.or(theme.0.idle_fg);
                    theme.0.info_bg = overrides.info_bg.or(theme.0.info_bg);
                    theme.0.info_fg = overrides.info_fg.or(theme.0.info_fg);
                    theme.0.good_bg = overrides.good_bg.or(theme.0.good_bg);
                    theme.0.good_fg = overrides.good_fg.or(theme.0.good_fg);
                    theme.0.warning_bg = overrides.warning_bg.or(theme.0.warning_bg);
                    theme.0.warning_fg = overrides.warning_fg.or(theme.0.warning_fg);
                    theme.0.critical_bg = overrides.critical_bg.or(theme.0.critical_bg);
                    theme.0.critical_fg = overrides.critical_fg.or(theme.0.critical_fg);
                    theme.0.separator = overrides.separator.or(theme.0.separator);
                    theme.0.separator_bg = overrides.separator_bg.or(theme.0.separator_bg);
                    theme.0.separator_fg = overrides.separator_fg.or(theme.0.separator_fg);
                    theme.0.alternating_tint_bg = overrides
                        .alternating_tint_bg
                        .or(theme.0.alternating_tint_bg);
                    theme.0.alternating_tint_fg = overrides
                        .alternating_tint_fg
                        .or(theme.0.alternating_tint_fg);
                }
                Ok(theme)
            }
        }

        deserializer.deserialize_any(ThemeVisitor)
    }
}

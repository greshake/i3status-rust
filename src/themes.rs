use std::default::Default;
use std::fmt;
use std::path::Path;

use lazy_static::lazy_static;
use serde_derive::Deserialize;

use serde::de::{self, Deserialize, Deserializer, MapAccess, Visitor};

use crate::util;

lazy_static! {
    pub static ref SLICK: Theme = Theme {
        native_separators: Some(false),
        idle_bg: Some(String::from("#424242")),
        idle_fg: Some(String::from("#ffffff")),
        info_bg: Some(String::from("#2196f3")),
        info_fg: Some(String::from("#ffffff")),
        good_bg: Some(String::from("#8bc34a")),
        good_fg: Some(String::from("#000000")),
        warning_bg: Some(String::from("#ffc107")),
        warning_fg: Some(String::from("#000000")),
        critical_bg: Some(String::from("#f44336")),
        critical_fg: Some(String::from("#ffffff")),
        separator: "\u{e0b2}".to_owned(),
        separator_bg: Some(String::from("auto")),
        separator_fg: Some(String::from("auto")),
        alternating_tint_bg: Some(String::from("#111111")),
        alternating_tint_fg: Some(String::from("#111111")),
    };

    pub static ref SOLARIZED_DARK: Theme = Theme {
        native_separators: Some(false),
        idle_bg: Some(String::from("#002b36")),      // base03
        idle_fg: Some(String::from("#93a1a1")),      // base1
        info_bg: Some(String::from("#268bd2")),      // blue
        info_fg: Some(String::from("#002b36")),      // base03
        good_bg: Some(String::from("#859900")),      // green
        good_fg: Some(String::from("#002b36")),      // base03
        warning_bg: Some(String::from("#b58900")),   // yellow
        warning_fg: Some(String::from("#002b36")),   // base03
        critical_bg: Some(String::from("#dc322f")),  // red
        critical_fg: Some(String::from("#002b36")),  // base03
        separator: "\u{e0b2}".to_owned(),
        separator_bg: Some(String::from("auto")),
        separator_fg: Some(String::from("auto")),
        alternating_tint_bg: None.to_owned(),
        alternating_tint_fg: None.to_owned(),
    };

    pub static ref SOLARIZED_LIGHT: Theme = Theme {
        native_separators: Some(false),
        idle_bg: Some(String::from("#fdf6e3")),      // base3
        idle_fg: Some(String::from("#586e75")),      // base01
        info_bg: Some(String::from("#268bd2")),      // blue
        info_fg: Some(String::from("#fdf6e3")),      // base3
        good_bg: Some(String::from("#859900")),      // green
        good_fg: Some(String::from("#fdf6e3")),      // base3
        warning_bg: Some(String::from("#b58900")),   // yellow
        warning_fg: Some(String::from("#fdf6e3")),   // base3
        critical_bg: Some(String::from("#dc322f")),  // red
        critical_fg: Some(String::from("#fdf6e3")),  // base3
        separator: "\u{e0b2}".to_owned(),
        separator_bg: Some(String::from("auto")),
        separator_fg: Some(String::from("auto")),
        alternating_tint_bg: None.to_owned(),
        alternating_tint_fg: None.to_owned(),
    };

    pub static ref MODERN: Theme = Theme {
        native_separators: Some(false),
        idle_bg: Some(String::from("#222D32")),
        idle_fg: Some(String::from("#CFD8DC")),
        info_bg: Some(String::from("#449CDB")),
        info_fg: Some(String::from("#1D1F21")),
        good_bg: Some(String::from("#99b938")),
        good_fg: Some(String::from("#1D1F21")),
        warning_bg: Some(String::from("#FE7E29")),
        warning_fg: Some(String::from("#1D1F21")),
        critical_bg: Some(String::from("#ff5252")),
        critical_fg: Some(String::from("#1D1F21")),
        separator: "\u{e0b2}".to_owned(),
        separator_bg: Some(String::from("auto")),
        separator_fg: Some(String::from("auto")),
        alternating_tint_bg: None.to_owned(),
        alternating_tint_fg: None.to_owned(),
    };

    pub static ref PLAIN: Theme = Theme {
        native_separators: Some(false),
        idle_bg: Some(String::from("#000000")),
        idle_fg: Some(String::from("#93a1a1")),
        info_bg: Some(String::from("#000000")),
        info_fg: Some(String::from("#93a1a1")),
        good_bg: Some(String::from("#000000")),
        good_fg: Some(String::from("#859900")),
        warning_bg: Some(String::from("#000000")),
        warning_fg: Some(String::from("#b58900")),
        critical_bg: Some(String::from("#000000")),
        critical_fg: Some(String::from("#dc322f")),
        separator: "|".to_owned(),
        separator_bg: Some(String::from("#000000")),
        separator_fg: Some(String::from("#a9a9a9")),
        alternating_tint_bg: None.to_owned(),
        alternating_tint_fg: None.to_owned(),
    };

    pub static ref BAD_WOLF: Theme = Theme {
        native_separators: Some(false),
        idle_bg: Some(String::from("#444444")),
        idle_fg: Some(String::from("#f5f5f5")),
        info_bg: Some(String::from("#626262")),
        info_fg: Some(String::from("#ffd680")),
        good_bg: Some(String::from("#afff00")),
        good_fg: Some(String::from("#000000")),
        warning_bg: Some(String::from("#ffaf00")),
        warning_fg: Some(String::from("#000000")),
        critical_bg: Some(String::from("#d70000")),
        critical_fg: Some(String::from("#000000")),
        separator: "\u{e0b2}".to_owned(),
        separator_bg: Some(String::from("auto")),
        separator_fg: Some(String::from("auto")),
        alternating_tint_bg: None.to_owned(),
        alternating_tint_fg: None.to_owned(),
    };

    pub static ref GRUVBOX_LIGHT: Theme = Theme {
        native_separators: Some(false),
        idle_bg: Some(String::from("#fbf1c7")),
        idle_fg: Some(String::from("#3c3836")),
        info_bg: Some(String::from("#458588")),
        info_fg: Some(String::from("#fbf1c7")),
        good_bg: Some(String::from("#98971a")),
        good_fg: Some(String::from("#fbf1c7")),
        warning_bg: Some(String::from("#d79921")),
        warning_fg: Some(String::from("#fbf1c7")),
        critical_bg: Some(String::from("#cc241d")),
        critical_fg: Some(String::from("#fbf1c7")),
        separator: "\u{e0b2}".to_owned(),
        separator_bg: Some(String::from("auto")),
        separator_fg: Some(String::from("auto")),
        alternating_tint_bg: None.to_owned(),
        alternating_tint_fg: None.to_owned(),
    };

    pub static ref GRUVBOX_DARK: Theme = Theme {
        native_separators: Some(false),
        idle_bg: Some(String::from("#282828")),
        idle_fg: Some(String::from("#ebdbb2")),
        info_bg: Some(String::from("#458588")),
        info_fg: Some(String::from("#ebdbb2")),
        good_bg: Some(String::from("#98971a")),
        good_fg: Some(String::from("#ebdbb2")),
        warning_bg: Some(String::from("#d79921")),
        warning_fg: Some(String::from("#ebdbb2")),
        critical_bg: Some(String::from("#cc241d")),
        critical_fg: Some(String::from("#ebdbb2")),
        separator: "\u{e0b2}".to_owned(),
        separator_bg: Some(String::from("auto")),
        separator_fg: Some(String::from("auto")),
        alternating_tint_bg: None.to_owned(),
        alternating_tint_fg: None.to_owned(),
    };

    pub static ref SPACE_VILLAIN: Theme = Theme {
        native_separators: Some(false),
        idle_bg: Some(String::from("#06060f")), //Rich black
        idle_fg: Some(String::from("#c1c1c1")), //Silver
        info_bg: Some(String::from("#00223f")), //Maastricht Blue
        info_fg: Some(String::from("#c1c1c1")), //Silver
        good_bg: Some(String::from("#394049")), //Arsenic
        good_fg: Some(String::from("#c1c1c1")), //Silver
        warning_bg: Some(String::from("#2d1637")), //Dark Purple
        warning_fg: Some(String::from("#c1c1c1")), //Silver
        critical_bg: Some(String::from("#c1c1c1")), //Silver
        critical_fg: Some(String::from("#2c1637")), //Dark Purple
        separator: "\u{e0b2}".to_owned(),
        separator_bg: Some(String::from("auto")),
        separator_fg: Some(String::from("auto")),
        alternating_tint_bg: None.to_owned(),
        alternating_tint_fg: None.to_owned(),
    };

    pub static ref SEMI_NATIVE: Theme = Theme {
        native_separators: Some(true),
        idle_bg: None.to_owned(),
        idle_fg: Some(String::from("#93a1a1")),
        info_bg: None.to_owned(),
        info_fg: Some(String::from("#93a1a1")),
        good_bg: None.to_owned(),
        good_fg: Some(String::from("#859900")),
        warning_bg: None.to_owned(),
        warning_fg: Some(String::from("#b58900")),
        critical_bg: None.to_owned(),
        critical_fg: Some(String::from("#dc322f")),
        separator: "".to_owned(),
        separator_bg: None.to_owned(),
        separator_fg: None.to_owned(),
        alternating_tint_bg: None.to_owned(),
        alternating_tint_fg: None.to_owned(),
    };

    pub static ref NATIVE: Theme = Theme {
        native_separators: Some(true),
        idle_bg: None.to_owned(),
        idle_fg: None.to_owned(),
        info_bg: None.to_owned(),
        info_fg: None.to_owned(),
        good_bg: None.to_owned(),
        good_fg: None.to_owned(),
        warning_bg: None.to_owned(),
        warning_fg: None.to_owned(),
        critical_bg: None.to_owned(),
        critical_fg: None.to_owned(),
        separator: "".to_owned(),
        separator_bg: None.to_owned(),
        separator_fg: None.to_owned(),
        alternating_tint_bg: None.to_owned(),
        alternating_tint_fg: None.to_owned(),
    };

}

#[derive(Debug, Clone)]
pub struct Theme {
    pub native_separators: Option<bool>,
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
    pub separator: String,
    pub separator_bg: Option<String>,
    pub separator_fg: Option<String>,
    pub alternating_tint_bg: Option<String>,
    pub alternating_tint_fg: Option<String>,
}

impl Default for Theme {
    fn default() -> Self {
        PLAIN.clone()
    }
}

impl Theme {
    pub fn from_name(name: &str) -> Option<Theme> {
        match name {
            "slick" => Some(SLICK.clone()),
            "solarized-dark" => Some(SOLARIZED_DARK.clone()),
            "solarized-light" => Some(SOLARIZED_LIGHT.clone()),
            "plain" => Some(PLAIN.clone()),
            "modern" => Some(MODERN.clone()),
            "bad-wolf" => Some(BAD_WOLF.clone()),
            "gruvbox-light" => Some(GRUVBOX_LIGHT.clone()),
            "gruvbox-dark" => Some(GRUVBOX_DARK.clone()),
            "space-villain" => Some(SPACE_VILLAIN.clone()),
            "semi-native" => Some(SEMI_NATIVE.clone()),
            "native" => Some(NATIVE.clone()),
            _ => None,
        }
    }

    pub fn from_file(file: &str) -> Option<Theme> {
        let full_path = Path::new(file);
        let xdg_path = util::xdg_config_home()
            .join("i3status-rust/themes")
            .join(file);
        let share_path = Path::new(util::USR_SHARE_PATH).join("themes").join(file);

        if full_path.exists() {
            util::deserialize_file(&full_path).ok()
        } else if xdg_path.exists() {
            util::deserialize_file(&xdg_path).ok()
        } else if share_path.exists() {
            util::deserialize_file(&share_path).ok()
        } else {
            None
        }
    }
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct ThemeOverrides {
    idle_bg: Option<String>,
    idle_fg: Option<String>,
    info_bg: Option<String>,
    info_fg: Option<String>,
    good_bg: Option<String>,
    good_fg: Option<String>,
    warning_bg: Option<String>,
    warning_fg: Option<String>,
    critical_bg: Option<String>,
    critical_fg: Option<String>,
    separator: Option<String>,
    separator_bg: Option<String>,
    separator_fg: Option<String>,
    alternating_tint_bg: Option<String>,
    alternating_tint_fg: Option<String>,
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
            fn visit_str<E>(self, name: &str) -> Result<Theme, E>
            where
                E: de::Error,
            {
                Theme::from_name(name)
                    .ok_or_else(|| de::Error::custom(format!("Theme \"{}\" not found.", name)))
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
                let mut overrides: Option<ThemeOverrides> = None;
                while let Some(key) = map.next_key()? {
                    match key {
                        Field::Name => {
                            if theme.is_some() {
                                return Err(de::Error::duplicate_field("name or file"));
                            }
                            let name = map.next_value()?;
                            theme = Some(Theme::from_name(name).ok_or_else(|| {
                                de::Error::custom(format!("Theme \"{}\" not found.", name))
                            })?);
                        }
                        Field::File => {
                            if theme.is_some() {
                                return Err(de::Error::duplicate_field("name or file"));
                            }
                            let file = map.next_value()?;
                            theme = Some(Theme::from_file(file).ok_or_else(|| {
                                de::Error::custom(format!(
                                    "Failed to load theme from file {}.",
                                    file
                                ))
                            })?);
                        }
                        Field::Overrides => {
                            if overrides.is_some() {
                                return Err(de::Error::duplicate_field("overrides"));
                            }
                            overrides = Some(map.next_value()?);
                        }
                    }
                }
                let mut theme = theme.unwrap_or_default();
                if let Some(overrides) = overrides {
                    theme.idle_bg = overrides.idle_bg.or(theme.idle_bg);
                    theme.idle_fg = overrides.idle_fg.or(theme.idle_fg);
                    theme.info_bg = overrides.info_bg.or(theme.info_bg);
                    theme.info_fg = overrides.info_fg.or(theme.info_fg);
                    theme.good_bg = overrides.good_bg.or(theme.good_bg);
                    theme.good_fg = overrides.good_fg.or(theme.good_fg);
                    theme.warning_bg = overrides.warning_bg.or(theme.warning_bg);
                    theme.warning_fg = overrides.warning_fg.or(theme.warning_fg);
                    theme.critical_bg = overrides.critical_bg.or(theme.critical_bg);
                    theme.critical_fg = overrides.critical_fg.or(theme.critical_fg);
                    theme.separator = overrides.separator.unwrap_or(theme.separator);
                    theme.separator_bg = overrides.separator_bg.or(theme.separator_bg);
                    theme.separator_fg = overrides.separator_fg.or(theme.separator_fg);
                    theme.alternating_tint_bg =
                        overrides.alternating_tint_bg.or(theme.alternating_tint_bg);
                    theme.alternating_tint_fg =
                        overrides.alternating_tint_fg.or(theme.alternating_tint_fg);
                }
                Ok(theme)
            }
        }

        deserializer.deserialize_any(ThemeVisitor)
    }
}

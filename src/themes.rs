use std::default::Default;

lazy_static! {
    pub static ref SLICK: Theme = Theme {
        idle_bg: "#424242".to_owned(),
        idle_fg: "#ffffff".to_owned(),
        info_bg: "#2196f3".to_owned(),
        info_fg: "#ffffff".to_owned(),
        good_bg: "#8bc34a".to_owned(),
        good_fg: "#000000".to_owned(),
        warning_bg: "#ffc107".to_owned(),
        warning_fg: "#000000".to_owned(),
        critical_bg: "#f44336".to_owned(),
        critical_fg: "#ffffff".to_owned(),
        separator: "\u{e0b2}".to_owned(),
        separator_bg: "auto".to_owned(),
        separator_fg: "auto".to_owned(),
        alternating_tint_bg: "#111111".to_owned(),
        alternating_tint_fg: "#111111".to_owned(),
    };

    pub static ref SOLARIZED_DARK: Theme = Theme {
        idle_bg: "#002b36".to_owned(),      // base03
        idle_fg: "#93a1a1".to_owned(),      // base1
        info_bg: "#268bd2".to_owned(),      // blue
        info_fg: "#002b36".to_owned(),      // base03
        good_bg: "#859900".to_owned(),      // green
        good_fg: "#002b36".to_owned(),      // base03
        warning_bg: "#b58900".to_owned(),   // yellow
        warning_fg: "#002b36".to_owned(),   // base03
        critical_bg: "#dc322f".to_owned(),  // red
        critical_fg: "#002b36".to_owned(),  // base03
        separator: "\u{e0b2}".to_owned(),
        separator_bg: "auto".to_owned(),
        separator_fg: "auto".to_owned(),
        alternating_tint_bg: "#000000".to_owned(),
        alternating_tint_fg: "#000000".to_owned(),
    };

    pub static ref SOLARIZED_LIGHT: Theme = Theme {
        idle_bg: "#fdf6e3".to_owned(),      // base3
        idle_fg: "#586e75".to_owned(),      // base01
        info_bg: "#268bd2".to_owned(),      // blue
        info_fg: "#fdf6e3".to_owned(),      // base3
        good_bg: "#859900".to_owned(),      // green
        good_fg: "#fdf6e3".to_owned(),      // base3
        warning_bg: "#b58900".to_owned(),   // yellow
        warning_fg: "#fdf6e3".to_owned(),   // base3
        critical_bg: "#dc322f".to_owned(),  // red
        critical_fg: "#fdf6e3".to_owned(),  // base3
        separator: "\u{e0b2}".to_owned(),
        separator_bg: "auto".to_owned(),
        separator_fg: "auto".to_owned(),
        alternating_tint_bg: "#000000".to_owned(),
        alternating_tint_fg: "#000000".to_owned(),
    };

    pub static ref MODERN: Theme = Theme {
        idle_bg: "#222D32".to_owned(),
        idle_fg: "#CFD8DC".to_owned(),
        info_bg: "#449CDB".to_owned(),
        info_fg: "#1D1F21".to_owned(),
        good_bg: "#99b938".to_owned(),
        good_fg: "#1D1F21".to_owned(),
        warning_bg: "#FE7E29".to_owned(),
        warning_fg: "#1D1F21".to_owned(),
        critical_bg: "#ff5252".to_owned(),
        critical_fg: "#1D1F21".to_owned(),
        separator: "\u{e0b2}".to_owned(),
        separator_bg: "auto".to_owned(),
        separator_fg: "auto".to_owned(),
        alternating_tint_bg: "#000000".to_owned(),
        alternating_tint_fg: "#000000".to_owned(),
    };

    pub static ref PLAIN: Theme = Theme {
        idle_bg: "#000000".to_owned(),
        idle_fg: "#93a1a1".to_owned(),
        info_bg: "#000000".to_owned(),
        info_fg: "#93a1a1".to_owned(),
        good_bg: "#000000".to_owned(),
        good_fg: "#859900".to_owned(),
        warning_bg: "#000000".to_owned(),
        warning_fg: "#b58900".to_owned(),
        critical_bg: "#000000".to_owned(),
        critical_fg: "#dc322f".to_owned(),
        separator: "| ".to_owned(),
        separator_bg: "#000000".to_owned(),
        separator_fg: "#a9a9a9".to_owned(),
        alternating_tint_bg: "#000000".to_owned(),
        alternating_tint_fg: "#000000".to_owned(),
    };

    pub static ref BAD_WOLF: Theme = Theme {
        idle_bg: "#444444".to_owned(),
        idle_fg: "#f5f5f5".to_owned(),
        info_bg: "#626262".to_owned(),
        info_fg: "#ffd680".to_owned(),
        good_bg: "#afff00".to_owned(),
        good_fg: "#000000".to_owned(),
        warning_bg: "#ffaf00".to_owned(),
        warning_fg: "#000000".to_owned(),
        critical_bg: "#d70000".to_owned(),
        critical_fg: "#000000".to_owned(),
        separator: "\u{e0b2}".to_owned(),
        separator_bg: "auto".to_owned(),
        separator_fg: "auto".to_owned(),
        alternating_tint_bg: "#000000".to_owned(),
        alternating_tint_fg: "#000000".to_owned(),
    };

    pub static ref GRUVBOX_LIGHT: Theme = Theme {
        idle_bg: "#fbf1c7".to_owned(),
        idle_fg: "#3c3836".to_owned(),
        info_bg: "#458588".to_owned(),
        info_fg: "#fbf1c7".to_owned(),
        good_bg: "#98971a".to_owned(),
        good_fg: "#fbf1c7".to_owned(),
        warning_bg: "#d79921".to_owned(),
        warning_fg: "#fbf1c7".to_owned(),
        critical_bg: "#cc241d".to_owned(),
        critical_fg: "#fbf1c7".to_owned(),
        separator: "\u{e0b2}".to_owned(),
        separator_bg: "auto".to_owned(),
        separator_fg: "auto".to_owned(),
        alternating_tint_bg: "#000000".to_owned(),
        alternating_tint_fg: "#000000".to_owned(),
    };

    pub static ref GRUVBOX_DARK: Theme = Theme {
        idle_bg: "#282828".to_owned(),
        idle_fg: "#ebdbb2".to_owned(),
        info_bg: "#458588".to_owned(),
        info_fg: "#ebdbb2".to_owned(),
        good_bg: "#98971a".to_owned(),
        good_fg: "#ebdbb2".to_owned(),
        warning_bg: "#d79921".to_owned(),
        warning_fg: "#ebdbb2".to_owned(),
        critical_bg: "#cc241d".to_owned(),
        critical_fg: "#ebdbb2".to_owned(),
        separator: "\u{e0b2}".to_owned(),
        separator_bg: "auto".to_owned(),
        separator_fg: "auto".to_owned(),
        alternating_tint_bg: "#000000".to_owned(),
        alternating_tint_fg: "#000000".to_owned(),
    };
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct Theme {
    pub idle_bg: String,
    pub idle_fg: String,
    pub info_bg: String,
    pub info_fg: String,
    pub good_bg: String,
    pub good_fg: String,
    pub warning_bg: String,
    pub warning_fg: String,
    pub critical_bg: String,
    pub critical_fg: String,
    pub separator: String,
    pub separator_bg: String,
    pub separator_fg: String,
    pub alternating_tint_bg: String,
    pub alternating_tint_fg: String,
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
            _ => None,
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

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct ThemeConfig {
    name: String,
    overrides: Option<ThemeOverrides>,
}

impl ThemeConfig {
    pub fn into_theme(self) -> Option<Theme> {
        let mut theme = Theme::from_name(&self.name)?;
        if let Some(overrides) = self.overrides {
            theme.idle_bg = overrides.idle_bg.unwrap_or(theme.idle_bg);
            theme.idle_fg = overrides.idle_fg.unwrap_or(theme.idle_fg);
            theme.info_bg = overrides.info_bg.unwrap_or(theme.info_bg);
            theme.info_fg = overrides.info_fg.unwrap_or(theme.info_fg);
            theme.warning_bg = overrides.warning_bg.unwrap_or(theme.warning_bg);
            theme.warning_fg = overrides.warning_fg.unwrap_or(theme.warning_fg);
            theme.critical_bg = overrides.critical_bg.unwrap_or(theme.critical_bg);
            theme.critical_fg = overrides.critical_fg.unwrap_or(theme.critical_fg);
            theme.separator = overrides.separator.unwrap_or(theme.separator);
            theme.separator_bg = overrides.separator_bg.unwrap_or(theme.separator_bg);
            theme.separator_fg = overrides.separator_fg.unwrap_or(theme.separator_fg);
            theme.alternating_tint_bg = overrides
                .alternating_tint_bg
                .unwrap_or(theme.alternating_tint_bg);
            theme.alternating_tint_fg = overrides
                .alternating_tint_fg
                .unwrap_or(theme.alternating_tint_fg);
        }
        Some(theme)
    }
}

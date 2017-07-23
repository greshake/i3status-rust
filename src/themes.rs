use std::str::FromStr;

lazy_static! {
    pub static ref SLICK: Theme = Theme {
        idle_bg: "#424242de".to_owned(),
        idle_fg: "#ffffff".to_owned(),
        info_bg: "#2196f3de".to_owned(),
        info_fg: "#ffffff".to_owned(),
        good_bg: "#8bc34ade".to_owned(),
        good_fg: "#000000de".to_owned(),
        warning_bg: "#ffc107de".to_owned(),
        warning_fg: "#000000de".to_owned(),
        critical_bg: "#f44336de".to_owned(),
        critical_fg: "#ffffff".to_owned(),
        separator: "\u{e0b2}".to_owned(),
        separator_bg: "auto".to_owned(),
        separator_fg: "auto".to_owned(),
        alternating_tint_bg: "#000000".to_owned(),
        alternating_tint_fg: "#000000".to_owned(),
    };

    pub static ref SOLARIZED_DARK: Theme = Theme {
        idle_bg: "#002b36".to_owned(),
        idle_fg: "#93a1a1".to_owned(),
        info_bg: "#268bd2".to_owned(),
        info_fg: "#002b36".to_owned(),
        good_bg: "#859900".to_owned(),
        good_fg: "#002b36".to_owned(),
        warning_bg: "#b58900".to_owned(),
        warning_fg: "#002b36".to_owned(),
        critical_bg: "#dc322f".to_owned(),
        critical_fg: "#002b36".to_owned(),
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
}

mapped_struct! {
    #[derive(Deserialize, Debug, Default, Clone)]
    #[serde(deny_unknown_fields)]
    pub struct Theme: String {
        pub idle_bg,
        pub idle_fg,
        pub info_bg,
        pub info_fg,
        pub good_bg,
        pub good_fg,
        pub warning_bg,
        pub warning_fg,
        pub critical_bg,
        pub critical_fg,
        pub separator,
        pub separator_bg,
        pub separator_fg,
        pub alternating_tint_bg,
        pub alternating_tint_fg
    }
}

impl FromStr for Theme {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        get_theme(s).ok_or_else(|| "unknown theme".into())
    }
}

pub fn get_theme(name: &str) -> Option<Theme> {
    match name {
        "slick" => Some(SLICK.clone()),
        "solarized-dark" => Some(SOLARIZED_DARK.clone()),
        "plain" => Some(PLAIN.clone()),
        "modern" => Some(MODERN.clone()),
        _ => None,
    }
}

pub fn default() -> Theme {
    PLAIN.clone()
}

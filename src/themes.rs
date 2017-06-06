use std::str::FromStr;

lazy_static! {
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
        separator: "".to_owned(),
        separator_bg: "auto".to_owned(),
        separator_fg: "auto".to_owned(),
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
        separator: "|".to_owned(),
        separator_bg: "#000000".to_owned(),
        separator_fg: "#a9a9a9".to_owned(),
    };
}

#[derive(Deserialize, Debug, Default, Clone)]
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
}

impl FromStr for Theme {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        get_theme(s)
            .ok_or_else(|| "unknown theme".into())
    }
}

pub fn get_theme(name: &str) -> Option<Theme> {
    match name {
        "solarized-dark" => Some(SOLARIZED_DARK.clone()),
        "plain" => Some(PLAIN.clone()),
        _ => None,
    }
}

pub fn default() -> Theme {
    PLAIN.clone()
}

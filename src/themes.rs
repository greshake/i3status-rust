#![warn(missing_docs)]

use serde_json::Value;

pub fn get_theme(name: &str) -> Option<Value> {
    match name {
        "solarized-dark" => Some(solarized_dark()),
        "plain" => Some(plain()),
        _ => None
    }
}

fn solarized_dark() -> Value {
    json!({
        "idle_bg": "#002b36",
        "idle_fg": "#93a1a1",
        "info_bg": "#268bd2",
        "info_fg": "#002b36",
        "good_bg": "#859900",
        "good_fg": "#002b36",
        "warning_bg": "#b58900",
        "warning_fg": "#002b36",
        "critical_bg": "#dc322f",
        "critical_fg": "#002b36",
        "separator": "",
        "separator_bg": "auto",
        "separator_fg": "auto",
    })
}

fn plain() -> Value {
    json!({
        "idle_bg": "#000000",
        "idle_fg": "#93a1a1",
        "info_bg": "#000000",
        "info_fg": "#93a1a1",
        "good_bg": "#000000",
        "good_fg": "#859900",
        "warning_bg": "#000000",
        "warning_fg": "#b58900",
        "critical_bg": "#000000",
        "critical_fg": "#dc322f",
        "separator": "|",
        "separator_bg": "#000000",
        "separator_fg": "#a9a9a9",
    })
}
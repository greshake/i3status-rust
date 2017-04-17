use serde_json::Value;

pub fn get_theme(name: &str) -> Option<Value> {
    match name {
        "solarized-dark" => Some(solarized_dark()),
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
        "critical_fg": "#002b36"
    })
}
use serde_json::Value;

pub fn get_icons(name: &str) -> Value {
    match name {
        "awesome" => awesome_icons(),
        _ => no_icons()
    }
}

fn no_icons() -> Value {
    json!({
        "time": ""
    })
}

fn awesome_icons() -> Value {
    json!({
        "time": "  "
    })
}

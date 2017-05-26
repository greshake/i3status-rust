use serde_json::Value;
use util::get_file;

pub fn get_icons(name: &str) -> Value {
    match name {
        "awesome" => awesome_icons(),
        "none" => no_icons(),
        s => ::serde_json::from_str(&get_file(s)).expect("Icon file is not valid JSON!"),
    }
}

fn no_icons() -> Value {
    let mut icons = json!({
        "": "",
        "time": " ",
        "music": " ",
        "music_play": ">",
        "music_pause": "||",
        "music_next": " > ",
        "music_prev": " < ",
        "cogs": " LOAD ",
        "memory_mem": " MEM ",
        "memory_swap": " SWAP ",
        "cpu": " CPU ",
        "bat": " BAT ",
        "bat_full": " FULL ",
        "bat_charging": " CHG ",
        "bat_discharging": " DCG ",
        "update": " UPD ",
        "toggle_off": " OFF ",
        "toggle_on": " ON ",
        "volume_full": " VOL ",
        "volume_half": " VOL ",
        "volume_empty": " VOL ",
        // This icon has no spaces around it because it is manually set as text. (sound.rs)
        "volume_muted": "MUTED",
        "thermometer": " TEMP ",
    });

    if cfg!(feature = "weather") {
        let mut icons = icons.as_object_mut().unwrap();
        let weather = [
            ("weather_sun", " OUT "),
            ("weather_snow", " OUT "),
            ("weather_thunder", " OUT "),
            ("weather_clouds", " OUT "),
            ("weather_rain", " OUT "),
            ("weather_default", " OUT ")
        ];
        for &(name, value) in weather.into_iter() {
            icons.insert(name.to_owned(), json!(value));
        }
    }

    icons
}

fn awesome_icons() -> Value {
    let mut icons = json!({
        "": "",
        "time": " \u{f017} ",
        "music": " \u{f001} ",
        "music_play": "  \u{f04b}  ",
        "music_pause": "  \u{f04c}  ",
        "music_next": " \u{f061} ",
        "music_prev": " \u{f060} ",
        "cogs": " \u{f085} ",
        "memory_mem": " \u{f2db} ",
        "memory_swap": " \u{f0a0} ",
        "cpu": " \u{f0e4} ",
        "bat": " \u{f242} ",
        "bat_full": " \u{f240} ",
        "bat_charging": " \u{f1e6} ",
        "bat_discharging": " \u{f242} ",
        "update": " \u{f062} ",
        "toggle_off": " \u{f204} ",
        "toggle_on": " \u{f205} ",
        "volume_full": " \u{f028} ",
        "volume_half": " \u{f027} ",
        "volume_empty": " \u{f026} ",
        // This icon has no spaces around it because it is manually set as text. (sound.rs)
        "volume_muted": "\u{f00d}",
        "thermometer": " \u{f2c8} "
    });

    if cfg!(feature = "weather") {
        let mut icons = icons.as_object_mut().unwrap();
        let weather = [
            ("weather_sun", " \u{f185} "),
            ("weather_snow", " \u{f2dc} "),
            ("weather_thunder", " \u{f0e7} "),
            ("weather_clouds", " \u{f0c2} "),
            ("weather_rain", " \u{f043} "),
            // Cloud symbol as default
            ("weather_default", " \u{f02c} ")
        ];
        for &(name, value) in weather.into_iter() {
            icons.insert(name.to_owned(), json!(value));
        }
    }

    icons
}

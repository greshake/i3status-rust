use std::collections::HashMap as Map;

lazy_static! {
    pub static ref NONE: Map<String, String> = map_to_owned! {
        "" => "",
        "time" => " ",
        "music" => " ",
        "music_play" => ">",
        "music_pause" => "||",
        "music_next" => " > ",
        "music_prev" => " < ",
        "cogs" => " LOAD ",
        "memory_mem" => " MEM ",
        "memory_swap" => " SWAP ",
        "cpu" => " CPU ",
        "bat" => " BAT ",
        "bat_full" => " FULL ",
        "bat_charging" => " CHG ",
        "bat_discharging" => " DCG ",
        "update" => " UPD ",
        "toggle_off" => " OFF ",
        "toggle_on" => " ON ",
        "volume_full" => " VOL ",
        "volume_half" => " VOL ",
        "volume_empty" => " VOL ",
        // This icon has no spaces around it because it is manually set as text. (sound.rs)
        "volume_muted" => "MUTED",
        "thermometer" => " TEMP ",
        "xrandr" => " SCREEN "
    };

    pub static ref AWESOME: Map<String, String> = map_to_owned! {
        "" => "",
        "time" => " \u{f017} ",
        "music" => " \u{f001} ",
        "music_play" => "  \u{f04b}  ",
        "music_pause" => "  \u{f04c}  ",
        "music_next" => " \u{f061} ",
        "music_prev" => " \u{f060} ",
        "cogs" => " \u{f085} ",
        "memory_mem" => " \u{f2db} ",
        "memory_swap" => " \u{f0a0} ",
        "cpu" => " \u{f0e4} ",
        "bat" => " \u{f242} ",
        "bat_full" => " \u{f240} ",
        "bat_charging" => " \u{f1e6} ",
        "bat_discharging" => " \u{f242} ",
        "update" => " \u{f062} ",
        "toggle_off" => " \u{f204} ",
        "toggle_on" => " \u{f205} ",
        "volume_full" => " \u{f028} ",
        "volume_half" => " \u{f027} ",
        "volume_empty" => " \u{f026} ",
        // This icon has no spaces around it because it is manually set as text. (sound.rs)
        "volume_muted" => "\u{f00d}",
        "thermometer" => " \u{f2c8} ",
        "xrandr" => " \u{f26c} "
    };
}

pub fn get_icons(name: &str) -> Option<Map<String, String>> {
    match name {
        "awesome" => Some(AWESOME.clone()),
        "none" => Some(NONE.clone()),
        _ => None,
    }
}

pub fn default() -> Map<String, String> {
    NONE.clone()
}

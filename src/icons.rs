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
        "xrandr" => " SCREEN ",
        "net_up" => " UP ",
        "net_down" => " DOWN ",
        "ping" => " PING "
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
        "xrandr" => " \u{f26c} ",
        "net_up" => " \u{2b06} ",
        "net_down" => " \u{2b07} ",
        "ping" => " \u{21ba} "
    };

    pub static ref MATERIAL: Map<String, String> = map_to_owned! {
        "" => "",
        "time" => " \u{e192} ",
        "music" => " \u{e405} ",
        "music_play" => "  \u{e037}  ",
        "music_pause" => "  \u{e034}  ",
        "music_next" => " \u{e044} ",
        "music_prev" => " \u{e045} ",
        "cogs" => " \u{e8b8} ",
        "memory_mem" => " \u{e322} ",
        "memory_swap" => " \u{e8d4} ",
        "cpu" => " \u{e640} ",
        "bat" => " \u{e1a5} ",
        "bat_full" => " \u{e1a4} ",
        "bat_charging" => " \u{e1a3} ",
        "bat_discharging" => " \u{e19c} ",
        "update" => " \u{e8d7} ",
        "toggle_off" => " \u{e836} ",
        "toggle_on" => " \u{e837} ",
        "volume_full" => " \u{e050} ",
        "volume_half" => " \u{e04d} ",
        "volume_empty" => " \u{e04e} ",
        // This icon has no spaces around it because it is manually set as text. (sound.rs)
        "volume_muted" => "\u{e04f}",
        "thermometer" => " \u{f2c8} ", // TODO
        "xrandr" => " \u{e31e} "
    };
}

pub fn get_icons(name: &str) -> Option<Map<String, String>> {
    match name {
        "material" => Some(MATERIAL.clone()),
        "awesome" => Some(AWESOME.clone()),
        "none" => Some(NONE.clone()),
        _ => None,
    }
}

pub fn default() -> Map<String, String> {
    NONE.clone()
}

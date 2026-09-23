use crate::errors::*;
use crate::util;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize, Debug, Clone)]
#[serde(try_from = "IconsConfigRaw")]
pub struct Icons(pub HashMap<String, Icon>);

#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum Icon {
    Single(String),
    Progression(Vec<String>),
}

impl From<&'static str> for Icon {
    fn from(value: &'static str) -> Self {
        Self::Single(value.into())
    }
}

impl<const N: usize> From<[&str; N]> for Icon {
    fn from(value: [&str; N]) -> Self {
        Self::Progression(value.iter().map(|s| s.to_string()).collect())
    }
}

/// Every icon key the bar knows. Blocks name icons through these consts,
/// and `ALL` lets the tests check the default set covers every key.
macro_rules! icon_keys {
    ($($name:ident = $key:literal,)*) => {
        $(pub(crate) const $name: &str = $key;)*

        #[cfg(test)]
        const ALL: &[&str] = &[$($name),*];
    };
}

icon_keys! {
    BACKLIGHT = "backlight",
    BAT = "bat",
    BAT_CHARGING = "bat_charging",
    BAT_NOT_AVAILABLE = "bat_not_available",
    BELL = "bell",
    BELL_SLASH = "bell-slash",
    BLUETOOTH = "bluetooth",
    CALENDAR = "calendar",
    COGS = "cogs",
    CPU = "cpu",
    CPU_BOOST_OFF = "cpu_boost_off",
    CPU_BOOST_ON = "cpu_boost_on",
    DISK_DRIVE = "disk_drive",
    DOCKER = "docker",
    GITHUB = "github",
    GPU = "gpu",
    HEADPHONES = "headphones",
    HUESHIFT = "hueshift",
    JOYSTICK = "joystick",
    KEYBOARD = "keyboard",
    MAIL = "mail",
    MEMORY_MEM = "memory_mem",
    MEMORY_SWAP = "memory_swap",
    MICROPHONE = "microphone",
    MICROPHONE_MUTED = "microphone_muted",
    MOUSE = "mouse",
    MUSIC = "music",
    MUSIC_NEXT = "music_next",
    MUSIC_PAUSE = "music_pause",
    MUSIC_PLAY = "music_play",
    MUSIC_PREV = "music_prev",
    NET_BRIDGE = "net_bridge",
    NET_CELLULAR = "net_cellular",
    NET_DOWN = "net_down",
    NET_LOOPBACK = "net_loopback",
    NET_MODEM = "net_modem",
    NET_UP = "net_up",
    NET_VPN = "net_vpn",
    NET_WIRED = "net_wired",
    NET_WIRELESS = "net_wireless",
    NOTIFICATION = "notification",
    PHONE = "phone",
    PHONE_DISCONNECTED = "phone_disconnected",
    PING = "ping",
    POMODORO = "pomodoro",
    POMODORO_BREAK = "pomodoro_break",
    POMODORO_PAUSED = "pomodoro_paused",
    POMODORO_STARTED = "pomodoro_started",
    POMODORO_STOPPED = "pomodoro_stopped",
    REFRESH = "refresh",
    RESOLUTION = "resolution",
    SCRATCHPAD = "scratchpad",
    TASKS = "tasks",
    TEA = "tea",
    THERMOMETER = "thermometer",
    TIME = "time",
    TOGGLE_OFF = "toggle_off",
    TOGGLE_ON = "toggle_on",
    UNKNOWN = "unknown",
    UPDATE = "update",
    UPTIME = "uptime",
    VOLUME = "volume",
    VOLUME_MUTED = "volume_muted",
    WEATHER_CLOUDS = "weather_clouds",
    WEATHER_CLOUDS_NIGHT = "weather_clouds_night",
    WEATHER_DEFAULT = "weather_default",
    WEATHER_FOG = "weather_fog",
    WEATHER_FOG_NIGHT = "weather_fog_night",
    WEATHER_MOON = "weather_moon",
    WEATHER_RAIN = "weather_rain",
    WEATHER_RAIN_NIGHT = "weather_rain_night",
    WEATHER_SNOW = "weather_snow",
    WEATHER_SUN = "weather_sun",
    WEATHER_THUNDER = "weather_thunder",
    WEATHER_THUNDER_NIGHT = "weather_thunder_night",
    WEBCAM = "webcam",
    XRANDR = "xrandr",
}

impl Default for Icons {
    fn default() -> Self {
        // "none" icon set
        Self(map! {
            BACKLIGHT => "BRIGHT",
            BAT => "BAT",
            BAT_CHARGING => "CHG",
            BAT_NOT_AVAILABLE => "BAT N/A",
            BELL => "ON",
            BELL_SLASH => "OFF",
            BLUETOOTH => "BT",
            CALENDAR => "CAL",
            COGS => "LOAD",
            CPU => "CPU",
            CPU_BOOST_ON => "BOOST ON",
            CPU_BOOST_OFF => "BOOST OFF",
            DISK_DRIVE => "DISK",
            DOCKER => "DOCKER",
            GITHUB => "GITHUB",
            GPU => "GPU",
            HEADPHONES => "HEAD",
            HUESHIFT => "HUE",
            JOYSTICK => "JOY",
            KEYBOARD => "KBD",
            MAIL => "MAIL",
            MEMORY_MEM => "MEM",
            MEMORY_SWAP => "SWAP",
            MOUSE => "MOUSE",
            MUSIC => "MUSIC",
            MUSIC_NEXT => ">",
            MUSIC_PAUSE => "||",
            MUSIC_PLAY => ">",
            MUSIC_PREV => "<",
            NET_BRIDGE => "BRIDGE",
            NET_CELLULAR => [
                                "NO SIGNAL",
                                "0 BARS",
                                "1 BAR",
                                "2 BARS",
                                "3 BARS",
                                "4 BARS",
                              ],
            NET_DOWN => "DOWN",
            NET_LOOPBACK => "LO",
            NET_MODEM => "MODEM",
            NET_UP => "UP ",
            NET_VPN => "VPN",
            NET_WIRED => "ETH",
            NET_WIRELESS => "WLAN",
            NOTIFICATION => "NOTIF",
            PHONE => "PHONE",
            PHONE_DISCONNECTED => "PHONE",
            PING => "PING",
            POMODORO => "POMODORO",
            POMODORO_BREAK => "BREAK",
            POMODORO_PAUSED => "PAUSED",
            POMODORO_STARTED => "STARTED",
            POMODORO_STOPPED => "STOPPED",
            REFRESH => "REFRESH",
            RESOLUTION => "RES",
            SCRATCHPAD => "[]",
            TASKS => "TSK",
            TEA => "TEA",
            THERMOMETER => "TEMP",
            TIME => "TIME",
            TOGGLE_OFF => "OFF",
            TOGGLE_ON => "ON",
            UNKNOWN => "??",
            UPDATE => "UPD",
            UPTIME => "UP",
            VOLUME => "VOL",
            VOLUME_MUTED => "VOL MUTED",
            MICROPHONE => "MIC",
            MICROPHONE_MUTED => "MIC MUTED",
            WEATHER_CLOUDS_NIGHT => "CLOUDY",
            WEATHER_CLOUDS => "CLOUDY",
            WEATHER_DEFAULT => "WEATHER",
            WEATHER_FOG_NIGHT => "FOG",
            WEATHER_FOG => "FOG",
            WEATHER_MOON => "MOONY",
            WEATHER_RAIN_NIGHT => "RAIN",
            WEATHER_RAIN => "RAIN",
            WEATHER_SNOW => "SNOW",
            WEATHER_SUN => "SUNNY",
            WEATHER_THUNDER_NIGHT => "STORM",
            WEATHER_THUNDER => "STORM",
            WEBCAM => "CAM",
            XRANDR => "SCREEN"
        })
    }
}

impl Icons {
    pub fn from_file(file: &str) -> Result<Self> {
        if file == "none" {
            Ok(Icons::default())
        } else {
            let file = util::find_file(file, Some("icons"), Some("toml"))?
                .or_error(|| format!("Icon set '{file}' not found"))?;
            Ok(Icons(util::deserialize_toml_file(file)?))
        }
    }

    pub fn apply_overrides(&mut self, overrides: HashMap<String, Icon>) {
        self.0.extend(overrides);
    }

    pub fn get(&self, icon: &'_ str, value: Option<f64>) -> Option<&str> {
        match (self.0.get(icon)?, value) {
            (Icon::Single(icon), _) => Some(icon),
            (Icon::Progression(prog), _) if prog.is_empty() => None,
            (Icon::Progression(prog), None) => Some(prog.last().unwrap()),
            (Icon::Progression(prog), Some(value)) => {
                let index = ((value * prog.len() as f64) as usize).clamp(0, prog.len() - 1);
                Some(prog[index].as_str())
            }
        }
    }
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, default)]
struct IconsConfigRaw {
    icons: Option<String>,
    overrides: Option<HashMap<String, Icon>>,
}

impl TryFrom<IconsConfigRaw> for Icons {
    type Error = Error;

    fn try_from(raw: IconsConfigRaw) -> Result<Self, Self::Error> {
        let mut icons = Self::from_file(raw.icons.as_deref().unwrap_or("none"))?;
        if let Some(overrides) = raw.overrides {
            for icon in overrides {
                icons.0.insert(icon.0, icon.1);
            }
        }
        Ok(icons)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_icons_cover_every_key() {
        let icons = Icons::default();
        let mut defaults: Vec<&str> = icons.0.keys().map(String::as_str).collect();
        let mut keys = ALL.to_vec();
        defaults.sort_unstable();
        keys.sort_unstable();
        assert_eq!(defaults, keys);
    }
}

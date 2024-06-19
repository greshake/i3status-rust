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

impl Default for Icons {
    fn default() -> Self {
        // "none" icon set
        Self(map! {
            "backlight" => "BRIGHT",
            "bat" => "BAT",
            "bat_charging" => "CHG",
            "bat_not_available" => "BAT N/A",
            "bell" => "ON",
            "bell-slash" => "OFF",
            "bluetooth" => "BT",
            "calendar" => "CAL",
            "cogs" => "LOAD",
            "cpu" => "CPU",
            "cpu_boost_on" => "BOOST ON",
            "cpu_boost_off" => "BOOST OFF",
            "disk_drive" => "DISK",
            "docker" => "DOCKER",
            "github" => "GITHUB",
            "gpu" => "GPU",
            "headphones" => "HEAD",
            "joystick" => "JOY",
            "keyboard" => "KBD",
            "mail" => "MAIL",
            "memory_mem" => "MEM",
            "memory_swap" => "SWAP",
            "mouse" => "MOUSE",
            "music" => "MUSIC",
            "music_next" => ">",
            "music_pause" => "||",
            "music_play" => ">",
            "music_prev" => "<",
            "net_bridge" => "BRIDGE",
            "net_cellular" => [
                                "NO SIGNAL",
                                "0 BARS",
                                "1 BAR",
                                "2 BARS",
                                "3 BARS",
                                "4 BARS",
                              ],
            "net_down" => "DOWN",
            "net_loopback" => "LO",
            "net_modem" => "MODEM",
            "net_up" => "UP ",
            "net_vpn" => "VPN",
            "net_wired" => "ETH",
            "net_wireless" => "WLAN",
            "notification" => "NOTIF",
            "phone" => "PHONE",
            "phone_disconnected" => "PHONE",
            "ping" => "PING",
            "pomodoro" => "POMODORO",
            "pomodoro_break" => "BREAK",
            "pomodoro_paused" => "PAUSED",
            "pomodoro_started" => "STARTED",
            "pomodoro_stopped" => "STOPPED",
            "resolution" => "RES",
            "scratchpad" => "[]",
            "tasks" => "TSK",
            "tea" => "TEA",
            "thermometer" => "TEMP",
            "time" => "TIME",
            "toggle_off" => "OFF",
            "toggle_on" => "ON",
            "unknown" => "??",
            "update" => "UPD",
            "uptime" => "UP",
            "volume" => "VOL",
            "volume_muted" => "VOL MUTED",
            "microphone" => "MIC",
            "microphone_muted" => "MIC MUTED",
            "weather_clouds_night" => "CLOUDY",
            "weather_clouds" => "CLOUDY",
            "weather_default" => "WEATHER",
            "weather_fog_night" => "FOG",
            "weather_fog" => "FOG",
            "weather_moon" => "MOONY",
            "weather_rain_night" => "RAIN",
            "weather_rain" => "RAIN",
            "weather_snow" => "SNOW",
            "weather_sun" => "SUNNY",
            "weather_thunder_night" => "STORM",
            "weather_thunder" => "STORM",
            "webcam" => "CAM",
            "xrandr" => "SCREEN"
        })
    }
}

impl Icons {
    pub fn from_file(file: &str) -> Result<Self> {
        if file == "none" {
            Ok(Icons::default())
        } else {
            let file = util::find_file(file, Some("icons"), Some("toml"))
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

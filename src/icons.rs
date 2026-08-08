use crate::errors::*;
use crate::util;
use serde::Deserialize;
use std::collections::HashMap;
#[cfg(test)]
use std::collections::HashSet;

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
            "hueshift" => "HUE",
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
            "refresh" => "REFRESH",
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

    /// The `i3status-icons` set is generated by `cargo xtask build-font` from
    /// the SVG sources. Guard the two things that silently break users if the
    /// generated file and the code drift apart.
    #[test]
    fn i3status_icons_set_matches_canonical_names() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/files/icons/i3status-icons.toml"
        );
        let icons: HashMap<String, Icon> =
            toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();

        let defaults = Icons::default();
        let canonical: HashSet<&str> = defaults.0.keys().map(String::as_str).collect();
        let provided: HashSet<&str> = icons.keys().map(String::as_str).collect();
        assert_eq!(
            canonical, provided,
            "i3status-icons.toml is out of sync with Icons::default(); \
             run `cargo xtask build-font`"
        );
    }

    /// Codepoints are a compatibility contract: they are baked into this file
    /// and into any `icons_overrides` users have written by codepoint. Adding
    /// an SVG that does not sort last renumbers the font underneath them.
    #[test]
    fn i3status_icons_codepoints_are_in_the_private_use_range() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/files/icons/i3status-icons.toml"
        );
        let icons: HashMap<String, Icon> =
            toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();

        let mut seen: HashMap<char, String> = HashMap::new();
        for (name, icon) in &icons {
            let values = match icon {
                Icon::Single(s) => vec![s.clone()],
                Icon::Progression(v) => v.clone(),
            };
            for value in values {
                let mut chars = value.chars();
                let c = chars.next().unwrap_or_else(|| panic!("{name}: empty icon"));
                assert!(
                    chars.next().is_none(),
                    "{name}: expected a single codepoint, got {value:?}"
                );
                assert!(
                    ('\u{e900}'..='\u{e962}').contains(&c),
                    "{name}: U+{:04X} is outside the font's range U+E900..=U+E962",
                    c as u32
                );
                if let Some(other) = seen.insert(c, name.clone()) {
                    panic!("{name} and {other} both map to U+{:04X}", c as u32);
                }
            }
        }
    }
}

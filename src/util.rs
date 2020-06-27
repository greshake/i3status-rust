use num_traits::{clamp, ToPrimitive};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::prelude::*;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::prelude::v1::String;
use std::process::Command;

use serde::de::DeserializeOwned;
use serde_derive::Deserialize;
use serde_json::value::Value;

use crate::blocks::Block;
use crate::config::Config;
use crate::errors::*;

#[derive(Copy, Clone, Debug, Deserialize, Eq, PartialEq)]
pub enum Prefix {
    None = 0,
    K = 1,
    M = 2,
    G = 3,
    T = 4,
}

impl Prefix {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "",
            Self::K => "K",
            Self::M => "M",
            Self::G => "G",
            Self::T => "T",
        }
    }

    pub fn from_str(val: &str) -> Option<Self> {
        match val {
            "none" => Some(Self::None),
            "K" => Some(Self::K),
            "M" => Some(Self::M),
            "G" => Some(Self::G),
            "T" => Some(Self::T),
            _ => None,
        }
    }

    pub fn factor(self) -> f64 {
        1024f64.powi(self as i32)
    }

    pub fn next(self) -> Option<Self> {
        let ordered = [Self::None, Self::K, Self::M, Self::G, Self::T];
        ordered
            .get(1 + ordered.iter().position(|x| *x == self).unwrap())
            .cloned()
    }
}

impl Default for Prefix {
    fn default() -> Self {
        Self::None
    }
}

pub const USR_SHARE_PATH: &str = "/usr/share/i3status-rust";

pub fn escape_pango_text(text: String) -> String {
    text.chars()
        .map(|x| match x {
            '&' => "&amp;".to_string(),
            '<' => "&lt;".to_string(),
            '>' => "&gt;".to_string(),
            '\'' => "&#39;".to_string(),
            _ => x.to_string(),
        })
        .collect()
}

pub fn battery_level_to_icon(charge_level: Result<u64>) -> &'static str {
    match charge_level {
        Ok(0..=5) => "bat_empty",
        Ok(6..=25) => "bat_quarter",
        Ok(26..=50) => "bat_half",
        Ok(51..=75) => "bat_three_quarters",
        _ => "bat_full",
    }
}

pub fn xdg_config_home() -> PathBuf {
    // In the unlikely event that $HOME is not set, it doesn't really matter
    // what we fall back on, so use /.config.
    let config_path = std::env::var("XDG_CONFIG_HOME").unwrap_or(format!(
        "{}/.config",
        std::env::var("HOME").unwrap_or_else(|_| "".to_string())
    ));
    PathBuf::from(&config_path)
}

pub fn deserialize_file<T>(file: &str) -> Result<T>
where
    T: DeserializeOwned,
{
    let mut contents = String::new();
    let mut file = BufReader::new(
        File::open(file).internal_error("util", &format!("failed to open file: {}", file))?,
    );
    file.read_to_string(&mut contents)
        .internal_error("util", "failed to read file")?;
    toml::from_str(&contents).configuration_error("failed to parse TOML from file contents")
}

pub fn read_file(blockname: &str, path: &Path) -> Result<String> {
    let mut f = OpenOptions::new().read(true).open(path).block_error(
        blockname,
        &format!("failed to open file {}", path.to_string_lossy()),
    )?;
    let mut content = String::new();
    f.read_to_string(&mut content).block_error(
        blockname,
        &format!("failed to read {}", path.to_string_lossy()),
    )?;
    // Removes trailing newline
    content.pop();
    Ok(content)
}

#[allow(dead_code)]
pub fn get_file(name: &str) -> Result<String> {
    let mut file_contents = String::new();
    let mut file = File::open(name).internal_error("util", &format!("Unable to open {}", name))?;
    file.read_to_string(&mut file_contents)
        .internal_error("util", &format!("Unable to read {}", name))?;
    Ok(file_contents)
}

pub fn has_command(block_name: &str, command: &str) -> Result<bool> {
    let exit_status = Command::new("sh")
        .args(&[
            "-c",
            format!("command -v {} >/dev/null 2>&1", command).as_ref(),
        ])
        .status()
        .block_error(
            block_name,
            format!("failed to start command to check for {}", command).as_ref(),
        )?;
    Ok(exit_status.success())
}

macro_rules! match_range {
    ($a:expr, default: ($default:expr) {$($lower:expr ; $upper:expr => $e:expr),+}) => (
        match $a {
            $(
                t if t >= $lower && t <= $upper => { $e },
            )+
            _ => { $default }
        }
    )
}

macro_rules! map (
    { $($key:expr => $value:expr),+ } => {
        {
            let mut m = ::std::collections::HashMap::new();
            $(
                m.insert($key, $value);
            )+
            m
        }
     };
);

macro_rules! map_to_owned (
    { $($key:expr => $value:expr),+ } => {
        {
            let mut m = ::std::collections::HashMap::new();
            $(
                m.insert($key.to_owned(), $value.to_owned());
            )+
            m
        }
     };
);

struct PrintState {
    pub last_bg: Option<String>,
    pub has_predecessor: bool,
}

impl PrintState {
    fn set_last_bg(&mut self, bg: String) {
        self.last_bg = Some(bg);
    }
    fn set_predecessor(&mut self, pre: bool) {
        self.has_predecessor = pre;
    }
}

pub fn print_blocks(
    order: &[String],
    block_map: &HashMap<String, &mut dyn Block>,
    config: &Config,
) -> Result<()> {
    let mut state = PrintState {
        has_predecessor: false,
        last_bg: None,
    };

    print!("[");
    for block_id in order {
        let block = &(*(block_map
            .get(block_id)
            .internal_error("util", "couldn't get block by id")?));
        let widgets = block.view();
        if widgets.is_empty() {
            continue;
        }
        let first = widgets[0];
        let color = first.get_rendered()["background"]
            .as_str()
            .internal_error("util", "couldn't get background color")?;

        let sep_fg = if config.theme.separator_fg == "auto" {
            color
        } else {
            &config.theme.separator_fg
        };

        let sep_bg = if config.theme.separator_bg == "auto" {
            state.last_bg.clone()
        } else {
            Some(config.theme.separator_bg.clone())
        };

        let separator = json!({
            "full_text": config.theme.separator,
            "separator": false,
            "separator_block_width": 0,
            "background": match sep_bg {
                Some(bg) => Value::String(bg),
                None => Value::Null
            },
            "color": sep_fg,
            "markup": "pango"
        });
        print!(
            "{}{},",
            if state.has_predecessor { "," } else { "" },
            separator.to_string()
        );
        print!("{}", first.to_string());
        state.set_last_bg(color.to_owned());
        state.set_predecessor(true);

        for widget in widgets.iter().skip(1) {
            print!(
                "{}{}",
                if state.has_predecessor { "," } else { "" },
                widget.to_string()
            );
            state.set_last_bg(String::from(
                widget.get_rendered()["background"]
                    .as_str()
                    .internal_error("util", "couldn't get background color")?,
            ));
            state.set_predecessor(true);
        }
    }
    println!("],");

    Ok(())
}

pub fn color_from_rgba(
    color: &str,
) -> ::std::result::Result<(u8, u8, u8, u8), Box<dyn std::error::Error>> {
    Ok((
        u8::from_str_radix(&color.get(1..3).ok_or("invalid rgba color")?, 16)?,
        u8::from_str_radix(&color.get(3..5).ok_or("invalid rgba color")?, 16)?,
        u8::from_str_radix(&color.get(5..7).ok_or("invalid rgba color")?, 16)?,
        u8::from_str_radix(&color.get(7..9).unwrap_or("FF"), 16)?,
    ))
}

pub fn color_to_rgba(color: (u8, u8, u8, u8)) -> String {
    format!(
        "#{:02X}{:02X}{:02X}{:02X}",
        color.0, color.1, color.2, color.3
    )
}

pub fn format_percent_bar(percent: f32) -> String {
    let percent = percent.min(100.0);
    let percent = percent.max(0.0);

    (0..10)
        .map(|index| {
            let bucket_min = (index * 10) as f32;
            let fraction = percent - bucket_min;
            //println!("Fraction: {}", fraction);
            if fraction < 1.25 {
                '\u{2581}' // 1/8 block for empty so the whole bar is always visible
            } else if fraction < 2.5 {
                '\u{2582}' // 2/8 block
            } else if fraction < 3.75 {
                '\u{2583}' // 3/8 block
            } else if fraction < 5.0 {
                '\u{2584}' // 4/8 block
            } else if fraction < 6.25 {
                '\u{2585}' // 5/8 block
            } else if fraction < 7.5 {
                '\u{2586}' // 6/8 block
            } else if fraction < 8.75 {
                '\u{2587}' // 7/8 block
            } else {
                '\u{2588}' // Full block
            }
        })
        .collect()
}

pub fn format_vec_to_bar_graph<T>(content: &[T], min: Option<T>, max: Option<T>) -> String
where
    T: Ord + ToPrimitive,
{
    let bars = [
        '\u{2581}', '\u{2582}', '\u{2583}', '\u{2584}', '\u{2585}', '\u{2586}', '\u{2587}',
        '\u{2588}',
    ];
    let min: f64 = match min {
        Some(x) => x.to_f64().unwrap(),
        None => content.iter().min().unwrap().to_f64().unwrap(),
    };
    let max: f64 = match max {
        Some(x) => x.to_f64().unwrap(),
        None => content.iter().max().unwrap().to_f64().unwrap(),
    };
    let extant = max - min;
    if extant.is_normal() {
        let length = bars.len() as f64 - 1.0;
        content
            .iter()
            .map(|x| {
                bars[((clamp(x.to_f64().unwrap(), min, max) - min) / extant * length) as usize]
            })
            .collect::<_>()
    } else {
        (0..content.len() - 1).map(|_| bars[0]).collect::<_>()
    }
}

// TODO: Allow for other non-additive tints
pub fn add_colors(a: &str, b: &str) -> ::std::result::Result<String, Box<dyn std::error::Error>> {
    let (r_a, g_a, b_a, a_a) = color_from_rgba(a)?;
    let (r_b, g_b, b_b, a_b) = color_from_rgba(b)?;

    Ok(color_to_rgba((
        r_a.saturating_add(r_b),
        g_a.saturating_add(g_b),
        b_a.saturating_add(b_b),
        a_a.saturating_add(a_b),
    )))
}

macro_rules! if_debug {
    ($x:block) => (if cfg!(debug_assertions) $x)
}

#[cfg(test)]
mod tests {
    use crate::util::{color_from_rgba, has_command};

    #[test]
    // we assume sh is always available
    fn test_has_command_ok() {
        let has_command = has_command("none", "sh");
        assert!(has_command.is_ok());
        let has_command = has_command.unwrap();
        assert!(has_command);
    }

    #[test]
    // we assume thequickbrownfoxjumpsoverthelazydog command does not exist
    fn test_has_command_err() {
        let has_command = has_command("none", "thequickbrownfoxjumpsoverthelazydog");
        assert!(has_command.is_ok());
        let has_command = has_command.unwrap();
        assert!(!has_command)
    }
    #[test]
    fn test_color_from_rgba() {
        let valid_rgb = "#AABBCC"; //rgb
        let rgba = color_from_rgba(valid_rgb);
        assert!(rgba.is_ok());
        assert_eq!(rgba.unwrap(), (0xAA, 0xBB, 0xCC, 0xFF));
        let valid_rgba = "#AABBCC00"; // rgba
        let rgba = color_from_rgba(valid_rgba);
        assert!(rgba.is_ok());
        assert_eq!(rgba.unwrap(), (0xAA, 0xBB, 0xCC, 0x00));
    }

    #[test]
    fn test_color_from_rgba_invalid() {
        let invalid = "invalid";
        let rgba = color_from_rgba(invalid);
        assert!(rgba.is_err());
        let invalid = "AA"; // too short
        let rgba = color_from_rgba(invalid);
        assert!(rgba.is_err());
        let invalid = "AABBCC"; // invalid rgba (missing #)
        let rgba = color_from_rgba(invalid);
        assert!(rgba.is_err());
    }
}

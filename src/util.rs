// TODO: Replace with clamp() once the feature is stable? Ideally, remove num_traits altogether.
use num_traits::{clamp, ToPrimitive};
use std::collections::HashMap;
use std::fmt::Display;
use std::fs::{File, OpenOptions};
use std::io::prelude::*;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::prelude::v1::String;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use regex::Regex;
use serde::de::DeserializeOwned;

use crate::blocks::Block;
use crate::config::Config;
use crate::errors::*;

pub const USR_SHARE_PATH: &str = "/usr/share/i3status-rust";

pub fn pseudo_uuid() -> usize {
    static ID: AtomicUsize = AtomicUsize::new(usize::MAX);
    ID.fetch_sub(1, Ordering::SeqCst)
}

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

/// Format `raw_value` to engineering notation
pub fn format_number(raw_value: f64, total_digits: usize, min_suffix: &str, unit: &str) -> String {
    let min_exp_level = match min_suffix {
        "T" => 4,
        "G" => 3,
        "M" => 2,
        "K" => 1,
        "1" => 0,
        "m" => -1,
        "u" => -2,
        "n" => -3,
        _ => -4,
    };

    // TODO: Replace with .clamp once the feature is stable
    let exp_level = (raw_value.log10().div_euclid(3.) as i32)
        .max(min_exp_level)
        .min(4);
    let value = raw_value / (10f64).powi(exp_level * 3);

    let suffix = match exp_level {
        4 => "T",
        3 => "G",
        2 => "M",
        1 => "K",
        0 => "",
        -1 => "m",
        -2 => "u",
        -3 => "n",
        _ => "p",
    };

    let total_digits = total_digits as isize;
    let decimals = (if value >= 100. {
        total_digits - 3
    } else if value >= 10. {
        total_digits - 2
    } else {
        total_digits - 1
    })
    .max(0);

    format!("{:.*}{}{}", decimals as usize, value, suffix, unit)
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

pub fn deserialize_file<T>(path: &Path) -> Result<T>
where
    T: DeserializeOwned,
{
    let file = path.to_str().unwrap();
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

pub fn print_blocks(blocks: &[Box<dyn Block>], config: &Config) -> Result<()> {
    let mut last_bg: Option<String> = None;

    let mut rendered_blocks = vec![];

    /* To always start with the same alternating tint on the right side of the
     * bar it is easiest to calculate the number of visible blocks here and
     * flip the starting tint if an even number of blocks is visible. This way,
     * the last block should always be untinted.
     */
    let visible_count = blocks
        .iter()
        .filter(|block| !block.view().is_empty())
        .count();

    let mut alternator = visible_count % 2 == 0;

    for block in blocks.iter() {
        let widgets = block.view();
        if widgets.is_empty() {
            continue;
        }

        // Get the final JSON from all the widgets for this block
        let mut rendered_widgets = widgets
            .iter()
            .map(|widget| {
                let mut w_json: serde_json::Value = widget.get_rendered().to_owned();
                if alternator {
                    // Apply tint for all widgets of every second block
                    *w_json.get_mut("background").unwrap() = json!(add_colors(
                        w_json["background"].as_str(),
                        config.theme.alternating_tint_bg.as_deref()
                    )
                    .unwrap());
                    *w_json.get_mut("color").unwrap() = json!(add_colors(
                        w_json["color"].as_str(),
                        config.theme.alternating_tint_fg.as_deref()
                    )
                    .unwrap());
                }
                w_json
            })
            .collect::<Vec<serde_json::Value>>();

        alternator = !alternator;

        if config.theme.native_separators == Some(true) {
            // Re-add native separator on last widget for native theme
            *rendered_widgets
                .last_mut()
                .unwrap()
                .get_mut("separator")
                .unwrap() = json!(null);
            *rendered_widgets
                .last_mut()
                .unwrap()
                .get_mut("separator_block_width")
                .unwrap() = json!(null);
        }

        // Serialize and concatenate widgets
        let block_str = rendered_widgets
            .iter()
            .map(|w| w.to_string())
            .collect::<Vec<String>>()
            .join(",");

        if config.theme.native_separators == Some(true) {
            // Skip separator block for native theme
            rendered_blocks.push(block_str.to_string());
            continue;
        }

        // The first widget's BG is used to get the FG color for the current separator
        let first_bg = rendered_widgets.first().unwrap()["background"]
            .as_str()
            .internal_error("util", "couldn't get background color")?;

        let sep_fg = if config.theme.separator_fg == Some("auto".to_string()) {
            Some(first_bg.to_string())
        } else {
            config.theme.separator_fg.clone()
        };

        // The separator's BG is the last block's last widget's BG
        let sep_bg = if config.theme.separator_bg == Some("auto".to_string()) {
            last_bg
        } else {
            config.theme.separator_bg.clone()
        };

        let separator = json!({
            "full_text": config.theme.separator,
            "separator": false,
            "separator_block_width": 0,
            "background": sep_bg,
            "color": sep_fg,
            "markup": "pango"
        });

        rendered_blocks.push(format!("{},{}", separator.to_string(), block_str));

        // The last widget's BG is used to get the BG color for the next separator
        last_bg = Some(
            rendered_widgets.last().unwrap()["background"]
                .as_str()
                .internal_error("util", "couldn't get background color")?
                .to_string(),
        );
    }

    println!("[{}],", rendered_blocks.join(","));

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

// TODO: Allow for other non-additive tints
pub fn add_colors(
    a: Option<&str>,
    b: Option<&str>,
) -> ::std::result::Result<Option<String>, Box<dyn std::error::Error>> {
    match (a, b) {
        (None, _) => Ok(None),
        (Some(a), None) => Ok(Some(a.to_string())),
        (Some(a), Some(b)) => {
            let (r_a, g_a, b_a, a_a) = color_from_rgba(a)?;
            let (r_b, g_b, b_b, a_b) = color_from_rgba(b)?;

            Ok(Some(color_to_rgba((
                r_a.saturating_add(r_b),
                g_a.saturating_add(g_b),
                b_a.saturating_add(b_b),
                a_a.saturating_add(a_b),
            ))))
        }
    }
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
    // (x * one eighth block) https://en.wikipedia.org/wiki/Block_Elements
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

#[derive(Debug, Clone)]
pub enum FormatTemplate {
    Str(String, Option<Box<FormatTemplate>>),
    Var(String, Option<Box<FormatTemplate>>),
}

impl FormatTemplate {
    pub fn from_string(s: &str) -> Result<FormatTemplate> {
        let s_as_bytes = s.as_bytes();

        //valid var tokens: {} containing any amount of alphanumericals
        let re = Regex::new(r"\{[a-zA-Z0-9_-]+?\}").internal_error("util", "invalid regex")?;

        let mut token_vec: Vec<FormatTemplate> = vec![];
        let mut start: usize = 0;

        for re_match in re.find_iter(&s) {
            if re_match.start() != start {
                let str_vec: Vec<u8> = (&s_as_bytes)[start..re_match.start()].to_vec();
                token_vec.push(FormatTemplate::Str(
                    String::from_utf8(str_vec)
                        .internal_error("util", "failed to convert string from UTF8")?,
                    None,
                ));
            }
            token_vec.push(FormatTemplate::Var(re_match.as_str().to_string(), None));
            start = re_match.end();
        }
        let str_vec: Vec<u8> = (&s_as_bytes)[start..].to_vec();
        token_vec.push(FormatTemplate::Str(
            String::from_utf8(str_vec)
                .internal_error("util", "failed to convert string from UTF8")?,
            None,
        ));
        let mut template: FormatTemplate = match token_vec.pop() {
            Some(token) => token,
            _ => FormatTemplate::Str("".to_string(), None),
        };
        while let Some(token) = token_vec.pop() {
            template = match token {
                FormatTemplate::Str(s, _) => FormatTemplate::Str(s, Some(Box::new(template))),
                FormatTemplate::Var(s, _) => FormatTemplate::Var(s, Some(Box::new(template))),
            }
        }
        Ok(template)
    }

    // TODO: Make this function tail-recursive for compiler optimization, also only use the version below, static_str
    pub fn render<T: Display>(&self, vars: &HashMap<String, T>) -> String {
        use self::FormatTemplate::*;
        let mut rendered = String::new();
        match *self {
            Str(ref s, ref next) => {
                rendered.push_str(s);
                if let Some(ref next) = *next {
                    rendered.push_str(&*next.render(vars));
                };
            }
            Var(ref key, ref next) => {
                rendered.push_str(&format!(
                    "{}",
                    vars.get(key)
                        .unwrap_or_else(|| panic!("Unknown placeholder in format string: {}", key))
                ));
                if let Some(ref next) = *next {
                    rendered.push_str(&*next.render(vars));
                };
            }
        };
        rendered
    }

    pub fn render_static_str<T: Display>(&self, vars: &HashMap<&str, T>) -> Result<String> {
        use self::FormatTemplate::*;
        let mut rendered = String::new();
        match *self {
            Str(ref s, ref next) => {
                rendered.push_str(s);
                if let Some(ref next) = *next {
                    rendered.push_str(&*next.render_static_str(vars)?);
                };
            }
            Var(ref key, ref next) => {
                rendered.push_str(&format!(
                    "{}",
                    vars.get(&**key).internal_error(
                        "util",
                        &format!("Unknown placeholder in format string: {}", key)
                    )?
                ));
                if let Some(ref next) = *next {
                    rendered.push_str(&*next.render_static_str(vars)?);
                };
            }
        };
        Ok(rendered)
    }
}

macro_rules! if_debug {
    ($x:block) => (if cfg!(debug_assertions) $x)
}

#[cfg(test)]
mod tests {
    use crate::util::{color_from_rgba, format_number, has_command};

    #[test]
    fn test_format_number() {
        assert_eq!(format_number(1.0, 3, "", "s"), "1.00s");
        assert_eq!(format_number(1.007, 3, "", "s"), "1.01s");
        assert_eq!(format_number(1.007, 4, "K", "s"), "0.001Ks");
        assert_eq!(format_number(1007., 3, "K", "s"), "1.01Ks");
        assert_eq!(format_number(107_000., 3, "", "s"), "107Ks");
        assert_eq!(format_number(107., 3, "", "s"), "107s");
        assert_eq!(format_number(0.000_123_123, 3, "", "N"), "123uN");
    }

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

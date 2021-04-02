use std::fs::{File, OpenOptions};
use std::io::prelude::*;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::prelude::v1::String;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde::de::DeserializeOwned;

use crate::errors::*;

pub const USR_SHARE_PATH: &str = "/usr/share/i3status-rust";

pub fn pseudo_uuid() -> usize {
    static ID: AtomicUsize = AtomicUsize::new(usize::MAX);
    ID.fetch_sub(1, Ordering::SeqCst)
}

/// Tries to find a file in standard locations:
/// - Fist try to find a file by full path
/// - Then try XDG_CONFIG_HOME
/// - Then try `~/.local/share/`
/// - Then try `/usr/share/`
///
/// Automaticaly append an extension if not presented.
pub fn find_file(file: &str, subdir: Option<&str>, extension: Option<&str>) -> Option<PathBuf> {
    // Set (or update) the extension
    let mut file = PathBuf::from(file);
    if let Some(extension) = extension {
        file.set_extension(extension);
    }

    // Try full path
    if file.exists() {
        return Some(file);
    }

    // Try XDG_CONFIG_HOME
    let mut xdg_config_path = xdg_config_home().join("i3status-rust");
    if let Some(subdir) = subdir {
        xdg_config_path = xdg_config_path.join(subdir);
    }
    xdg_config_path = xdg_config_path.join(&file);
    if xdg_config_path.exists() {
        return Some(xdg_config_path);
    }

    // Try `~/.local/share/`
    if let Ok(home) = std::env::var("HOME") {
        let mut local_share_path = PathBuf::from(home).join(".local/share/i3status-rust");
        if let Some(subdir) = subdir {
            local_share_path = local_share_path.join(subdir);
        }
        local_share_path = local_share_path.join(&file);
        if local_share_path.exists() {
            return Some(local_share_path);
        }
    }

    // Try `/usr/share/`
    let mut usr_share_path = PathBuf::from(USR_SHARE_PATH);
    if let Some(subdir) = subdir {
        usr_share_path = usr_share_path.join(subdir);
    }
    usr_share_path = usr_share_path.join(&file);
    if usr_share_path.exists() {
        return Some(usr_share_path);
    }

    None
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
    PathBuf::from(std::env::var("XDG_CONFIG_HOME").unwrap_or(format!(
        "{}/.config",
        std::env::var("HOME").unwrap_or_default()
    )))
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

macro_rules! map {
    ($($key:expr => $value:expr),+ $(,)*) => {{
        let mut m = ::std::collections::HashMap::new();
        $(m.insert($key, $value);)+
        m
    }};
}

macro_rules! map_to_owned {
    { $($key:expr => $value:expr),+ } => {
        {
            let mut m = ::std::collections::HashMap::new();
            $(
                m.insert($key.to_owned(), $value.to_owned());
            )+
            m
        }
     };
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

pub fn format_vec_to_bar_graph(content: &[f64], min: Option<f64>, max: Option<f64>) -> String {
    // (x * one eighth block) https://en.wikipedia.org/wiki/Block_Elements
    static BARS: [char; 8] = [
        '\u{2581}', '\u{2582}', '\u{2583}', '\u{2584}', '\u{2585}', '\u{2586}', '\u{2587}',
        '\u{2588}',
    ];

    // Find min and max
    let mut min_v = std::f64::INFINITY;
    let mut max_v = -std::f64::INFINITY;
    for v in content {
        if *v < min_v {
            min_v = *v;
        }
        if *v > max_v {
            max_v = *v;
        }
    }

    let min = min.unwrap_or(min_v);
    let max = max.unwrap_or(max_v);
    let extant = max - min;
    if extant.is_normal() {
        let length = BARS.len() as f64 - 1.0;
        content
            .iter()
            .map(|x| BARS[((x.clamp(min, max) - min) / extant * length) as usize])
            .collect()
    } else {
        (0..content.len()).map(|_| BARS[0]).collect::<_>()
    }
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

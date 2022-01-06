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

pub fn escape_pango_text(text: &str) -> String {
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

pub fn battery_level_to_icon(charge_level: Result<u64>, fallback_icons: bool) -> &'static str {
    // TODO remove fallback in next release
    if fallback_icons {
        match charge_level {
            Ok(0..=5) => "bat_empty",
            Ok(6..=25) => "bat_quarter",
            Ok(26..=50) => "bat_half",
            Ok(51..=75) => "bat_three_quarters",
            _ => "bat_full",
        }
    } else {
        match charge_level {
            Ok(0..=10) => "bat_10",
            Ok(11..=20) => "bat_20",
            Ok(21..=30) => "bat_30",
            Ok(31..=40) => "bat_40",
            Ok(41..=50) => "bat_50",
            Ok(51..=60) => "bat_60",
            Ok(61..=70) => "bat_70",
            Ok(71..=80) => "bat_80",
            Ok(81..=90) => "bat_90",
            _ => "bat_full",
        }
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
    ($($key:expr => $value:expr),+ $(,)*) => {{
        let mut m = ::std::collections::HashMap::new();
        $(m.insert($key.to_owned(), $value.to_owned());)+
        m
    }};
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

// Convert 2 letter country code to Unicode
pub fn country_flag_from_iso_code(country_code: &str) -> String {
    if country_code.len() != 2 || !country_code.chars().all(|c| c.is_ascii_uppercase()) {
        return country_code.to_string();
    }
    let bytes = country_code.as_bytes(); // Sane as we verified before that it's ASCII

    // Each char is encoded as 1F1E6 to 1F1FF for A-Z
    let c1 = bytes[0] + 0xa5;
    let c2 = bytes[1] + 0xa5;
    // The last byte will always start with 101 (0xa0) and then the 5 least
    // significant bits from the previous result
    let b1 = 0xa0 | (c1 & 0x1f);
    let b2 = 0xa0 | (c2 & 0x1f);
    // Get the flag string from the UTF-8 representation of our Unicode characters.
    String::from_utf8(vec![0xf0, 0x9f, 0x87, b1, 0xf0, 0x9f, 0x87, b2]).unwrap()
}

#[cfg(test)]
mod tests {
    use crate::util::{country_flag_from_iso_code, has_command};

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
    fn test_flags() {
        assert!(country_flag_from_iso_code("ES") == "🇪🇸");
        assert!(country_flag_from_iso_code("US") == "🇺🇸");
        assert!(country_flag_from_iso_code("USA") == "USA");
    }
}

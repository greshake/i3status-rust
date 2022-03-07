use std::env;
use std::fs::File;
use std::io::prelude::*;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::prelude::v1::String;

use serde::de::DeserializeOwned;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

use crate::errors::*;

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
    if let Some(xdg_config) = xdg_config_home() {
        let mut xdg_config = xdg_config.join("i3status-rust");
        if let Some(subdir) = subdir {
            xdg_config = xdg_config.join(subdir);
        }
        xdg_config = xdg_config.join(&file);
        if xdg_config.exists() {
            return Some(xdg_config);
        }
    }

    // Try `~/.local/share/`
    if let Ok(home) = env::var("HOME") {
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
    let mut usr_share_path = PathBuf::from("/usr/share/i3status-rust");
    if let Some(subdir) = subdir {
        usr_share_path = usr_share_path.join(subdir);
    }
    usr_share_path = usr_share_path.join(&file);
    if usr_share_path.exists() {
        return Some(usr_share_path);
    }

    None
}

pub async fn new_dbus_connection() -> Result<zbus::Connection> {
    zbus::Connection::session()
        .await
        .error("Failed to open DBus session connection")
}

pub async fn new_system_dbus_connection() -> Result<zbus::Connection> {
    zbus::Connection::system()
        .await
        .error("Failed to open DBus system connection")
}

pub fn battery_level_icon(level: u8, charging: bool) -> &'static str {
    match (level, charging) {
        // TODO: use different charging icons
        (_, true) => "bat_charging",
        (0..=10, _) => "bat_10",
        (11..=20, _) => "bat_20",
        (21..=30, _) => "bat_30",
        (31..=40, _) => "bat_40",
        (41..=50, _) => "bat_50",
        (51..=60, _) => "bat_60",
        (61..=70, _) => "bat_70",
        (71..=80, _) => "bat_80",
        (81..=90, _) => "bat_90",
        _ => "bat_full",
    }
}

pub fn xdg_config_home() -> Option<PathBuf> {
    // If XDG_CONFIG_HOME is not set, fall back to use HOME/.config
    env::var("XDG_CONFIG_HOME")
        .ok()
        .or_else(|| {
            env::var("HOME")
                .ok()
                .map(|home| format!("{}/.config", home))
        })
        .map(PathBuf::from)
}

pub fn deserialize_toml_file<T>(path: &Path) -> Result<T>
where
    T: DeserializeOwned,
{
    let mut contents = String::new();
    let file = File::open(path).or_error(|| format!("Failed to open file: {}", path.display()))?;
    BufReader::new(file)
        .read_to_string(&mut contents)
        .or_error(|| format!("Failed to read file: {}", path.display()))?;
    toml::from_str(&contents).or_error(|| format!("Failed to deserialize file: {}", path.display()))
}

pub async fn read_file(path: impl AsRef<Path>) -> StdResult<String, std::io::Error> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut content = String::new();
    file.read_to_string(&mut content).await?;
    Ok(content.trim_end().to_string())
}

pub async fn has_command(command: &str) -> Result<bool> {
    Command::new("sh")
        .args(&[
            "-c",
            format!("command -v {} >/dev/null 2>&1", command).as_ref(),
        ])
        .status()
        .await
        .or_error(|| format!("Failed to check {} presence", command))
        .map(|status| status.success())
}

macro_rules! map {
    ($($key:expr => $value:expr),* $(,)*) => {{
        #[allow(unused_mut)]
        let mut m = ::std::collections::HashMap::new();
        $(m.insert($key.into(), $value.into());)*
        m
    }};
}

// macro_rules! regex {
//     ($re:literal $(,)?) => {{
//         static RE: once_cell::sync::OnceCell<regex::Regex> = once_cell::sync::OnceCell::new();
//         RE.get_or_init(|| regex::Regex::new($re).unwrap())
//     }};
// }

pub fn format_vec_to_bar_graph(content: &[f64]) -> smartstring::alias::String {
    // (x * one eighth block) https://en.wikipedia.org/wiki/Block_Elements
    static BARS: [char; 8] = [
        '\u{2581}', '\u{2582}', '\u{2583}', '\u{2584}', '\u{2585}', '\u{2586}', '\u{2587}',
        '\u{2588}',
    ];

    // Find min and max
    let mut min = std::f64::INFINITY;
    let mut max = -std::f64::INFINITY;
    for v in content {
        if *v < min {
            min = *v;
        }
        if *v > max {
            max = *v;
        }
    }

    let range = max - min;
    content
        .iter()
        .map(|x| BARS[((x - min) / range * 7.).clamp(0., 7.) as usize])
        .collect()
}

/// Convert 2 letter country code to Unicode
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
    use super::*;

    #[test]
    fn test_has_command_ok() {
        // we assume sh is always available
        assert!(tokio_test::block_on(has_command("sh")).unwrap());
    }

    #[test]
    fn test_has_command_err() {
        // we assume thequickbrownfoxjumpsoverthelazydog command does not exist
        assert!(!tokio_test::block_on(has_command("thequickbrownfoxjumpsoverthelazydog")).unwrap());
    }

    #[test]
    fn test_flags() {
        assert!(country_flag_from_iso_code("ES") == "🇪🇸");
        assert!(country_flag_from_iso_code("US") == "🇺🇸");
        assert!(country_flag_from_iso_code("USA") == "USA");
    }
}

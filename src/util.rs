use std::path::{Path, PathBuf};

use dirs::{config_dir, data_dir};
use serde::de::DeserializeOwned;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

use crate::errors::*;

/// Tries to find a file in standard locations:
/// - Fist try to find a file by full path (only if path is absolute)
/// - Then try XDG_CONFIG_HOME (e.g. `~/.config`)
/// - Then try XDG_DATA_HOME (e.g. `~/.local/share/`)
/// - Then try `/usr/share/`
///
/// Automatically append an extension if not presented.
pub fn find_file(file: &str, subdir: Option<&str>, extension: Option<&str>) -> Option<PathBuf> {
    let file = Path::new(file);

    if file.is_absolute() && file.exists() {
        return Some(file.to_path_buf());
    }

    // Try XDG_CONFIG_HOME (e.g. `~/.config`)
    if let Some(mut xdg_config) = config_dir() {
        xdg_config.push("i3status-rust");
        if let Some(subdir) = subdir {
            xdg_config.push(subdir);
        }
        xdg_config.push(file);
        if let Some(file) = exists_with_opt_extension(&xdg_config, extension) {
            return Some(file);
        }
    }

    // Try XDG_DATA_HOME (e.g. `~/.local/share/`)
    if let Some(mut xdg_data) = data_dir() {
        xdg_data.push("i3status-rust");
        if let Some(subdir) = subdir {
            xdg_data.push(subdir);
        }
        xdg_data.push(file);
        if let Some(file) = exists_with_opt_extension(&xdg_data, extension) {
            return Some(file);
        }
    }

    // Try `/usr/share/`
    let mut usr_share_path = PathBuf::from("/usr/share/i3status-rust");
    if let Some(subdir) = subdir {
        usr_share_path.push(subdir);
    }
    usr_share_path.push(file);
    if let Some(file) = exists_with_opt_extension(&usr_share_path, extension) {
        return Some(file);
    }

    None
}

fn exists_with_opt_extension(file: &Path, extension: Option<&str>) -> Option<PathBuf> {
    if file.exists() {
        return Some(file.into());
    }
    // If file has no extension, test with given extension
    if let (None, Some(extension)) = (file.extension(), extension) {
        let file = file.with_extension(extension);
        // Check again with extension added
        if file.exists() {
            return Some(file);
        }
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

pub fn deserialize_toml_file<T, P>(path: P) -> Result<T>
where
    T: DeserializeOwned,
    P: AsRef<Path>,
{
    let path = path.as_ref();

    let contents = std::fs::read_to_string(path)
        .or_error(|| format!("Failed to read file: {}", path.display()))?;

    toml::from_str(&contents).map_err(|err| {
        let location_msg = err
            .span()
            .map(|span| {
                let line = 1 + contents.as_bytes()[..(span.start)]
                    .iter()
                    .filter(|b| **b == b'\n')
                    .count();
                format!(" at line {line}")
            })
            .unwrap_or_default();
        Error::new(format!(
            "Failed to deserialize TOML file {}{}: {}",
            path.display(),
            location_msg,
            err.message()
        ))
    })
}

pub async fn read_file(path: impl AsRef<Path>) -> std::io::Result<String> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut content = String::new();
    file.read_to_string(&mut content).await?;
    Ok(content.trim_end().to_string())
}

pub async fn has_command(command: &str) -> Result<bool> {
    Command::new("sh")
        .args([
            "-c",
            format!("command -v {command} >/dev/null 2>&1").as_ref(),
        ])
        .status()
        .await
        .or_error(|| format!("Failed to check {command} presence"))
        .map(|status| status.success())
}

/// # Example
///
/// ```ignore
/// let opt = Some(1);
/// let m: HashMap<&'static str, String> = map! {
///     "key" => "value",
///     [if true] "hello" => "world",
///     [if let Some(x) = opt] "opt" => x.to_string(),
/// };
/// map! { @extend m
///     "new key" => "new value",
///     "one" => "more",
/// }
/// ```
#[macro_export]
macro_rules! map {
    (@extend $map:ident $( $([$($cond_tokens:tt)*])? $key:literal => $value:expr ),* $(,)?) => {{
        $(
        map!(@insert $map, $key, $value $(,$($cond_tokens)*)?);
        )*
    }};
    (@extend $map:ident $( $key:expr => $value:expr ),* $(,)?) => {{
        $(
        map!(@insert $map, $key, $value);
        )*
    }};
    (@insert $map:ident, $key:expr, $value:expr) => {{
        $map.insert($key.into(), $value.into());
    }};
    (@insert $map:ident, $key:expr, $value:expr, if $cond:expr) => {{
        if $cond {
        $map.insert($key.into(), $value.into());
        }
    }};
    (@insert $map:ident, $key:expr, $value:expr, if let $pat:pat = $match_on:expr) => {{
        if let $pat = $match_on {
        $map.insert($key.into(), $value.into());
        }
    }};
    ($($tt:tt)*) => {{
        #[allow(unused_mut)]
        let mut m = ::std::collections::HashMap::new();
        map!(@extend m $($tt)*);
        m
    }};
}

pub use map;

macro_rules! regex {
    ($re:literal $(,)?) => {{
        static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
        RE.get_or_init(|| regex::Regex::new($re).unwrap())
    }};
}

macro_rules! make_log_macro {
    (@wdoll $macro_name:ident, $block_name:literal, ($dol:tt)) => {
        #[allow(dead_code)]
        macro_rules! $macro_name {
            ($dol($args:tt)+) => {
                ::log::$macro_name!(target: $block_name, $dol($args)+);
            };
        }
    };
    ($macro_name:ident, $block_name:literal) => {
        make_log_macro!(@wdoll $macro_name, $block_name, ($));
    };
}

pub fn format_bar_graph(content: &[f64]) -> String {
    // (x * one eighth block) https://en.wikipedia.org/wiki/Block_Elements
    static BARS: [char; 8] = [
        '\u{2581}', '\u{2582}', '\u{2583}', '\u{2584}', '\u{2585}', '\u{2586}', '\u{2587}',
        '\u{2588}',
    ];

    // Find min and max
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for &v in content {
        min = min.min(v);
        max = max.max(v);
    }

    let range = max - min;
    content
        .iter()
        .map(|x| BARS[((x - min) / range * 7.).clamp(0., 7.) as usize])
        .collect()
}

/// Convert 2 letter country code to Unicode
pub fn country_flag_from_iso_code(country_code: &str) -> String {
    let [mut b1, mut b2]: [u8; 2] = country_code.as_bytes().try_into().unwrap_or([0, 0]);

    if !b1.is_ascii_uppercase() || !b2.is_ascii_uppercase() {
        return country_code.into();
    }

    // Each char is encoded as 1F1E6 to 1F1FF for A-Z
    b1 += 0xa5;
    b2 += 0xa5;
    // The last byte will always start with 101 (0xa0) and then the 5 least
    // significant bits from the previous result
    b1 = 0xa0 | (b1 & 0x1f);
    b2 = 0xa0 | (b2 & 0x1f);
    // Get the flag string from the UTF-8 representation of our Unicode characters.
    String::from_utf8(vec![0xf0, 0x9f, 0x87, b1, 0xf0, 0x9f, 0x87, b2]).unwrap()
}

/// A shortcut for `Default::default()`
/// See <https://github.com/rust-lang/rust/issues/73014>
#[inline]
pub fn default<T: Default>() -> T {
    Default::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_has_command_ok() {
        // we assume sh is always available
        assert!(has_command("sh").await.unwrap());
    }

    #[tokio::test]
    async fn test_has_command_err() {
        // we assume thequickbrownfoxjumpsoverthelazydog command does not exist
        assert!(!has_command("thequickbrownfoxjumpsoverthelazydog")
            .await
            .unwrap());
    }

    #[test]
    fn test_flags() {
        assert!(country_flag_from_iso_code("ES") == "🇪🇸");
        assert!(country_flag_from_iso_code("US") == "🇺🇸");
        assert!(country_flag_from_iso_code("USA") == "USA");
    }
}

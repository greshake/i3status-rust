use std::path::{Path, PathBuf};

use dirs::{config_dir, data_dir};
use serde::de::DeserializeOwned;
use tokio::io::AsyncReadExt as _;
use tokio::process::Command;

use crate::errors::*;

/// The list of paths `find_file` checks, in the order it checks them.
///
/// - An absolute path is tried as given.
/// - A relative path is tried inside XDG_CONFIG_HOME (e.g. `~/.config`), then
///   XDG_DATA_HOME (e.g. `~/.local/share/`), then `/usr/share/`, each with the
///   `i3status-rust` directory (and `subdir`, if given) appended.
/// - A path without an extension is also tried with `extension` appended.
pub fn file_candidates(file: &str, subdir: Option<&str>, extension: Option<&str>) -> Vec<PathBuf> {
    let file = Path::new(file);

    let mut bases: Vec<PathBuf> = Vec::new();
    if file.is_absolute() {
        bases.push(file.into());
    } else {
        for dir in [config_dir(), data_dir(), Some("/usr/share".into())]
            .into_iter()
            .flatten()
        {
            let mut base: PathBuf = dir;
            base.push("i3status-rust");
            if let Some(subdir) = subdir {
                base.push(subdir);
            }
            base.push(file);
            bases.push(base);
        }
    }

    let mut candidates = Vec::new();
    for base in bases {
        let with_extension = match (base.extension(), extension) {
            (None, Some(extension)) => Some(base.with_extension(extension)),
            _ => None,
        };
        candidates.push(base);
        candidates.extend(with_extension);
    }
    candidates
}

/// Tries to find a file in standard locations (see [`file_candidates`]).
pub fn find_file(
    file: &str,
    subdir: Option<&str>,
    extension: Option<&str>,
) -> Result<Option<PathBuf>> {
    for candidate in file_candidates(file, subdir, extension) {
        if candidate.try_exists().error("Unable to stat file")? {
            return Ok(Some(candidate));
        }
    }
    Ok(None)
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

    deserialize_toml_file_string(contents, path)
}

pub async fn async_deserialize_toml_file<T, P>(path: P) -> Result<T>
where
    T: DeserializeOwned,
    P: AsRef<Path>,
{
    let path = path.as_ref();

    let contents = read_file(path)
        .await
        .or_error(|| format!("Failed to read file: {}", path.display()))?;

    deserialize_toml_file_string(contents, path)
}

fn deserialize_toml_file_string<T>(contents: String, path: &Path) -> Result<T>
where
    T: DeserializeOwned,
{
    toml::from_str(&contents).map_err(|err| {
        let location_msg = err
            .span()
            .map(|span| {
                if span == (0..0) {
                    String::new()
                } else {
                    let line = 1 + contents.as_bytes()[..(span.start)]
                        .iter()
                        .filter(|b| **b == b'\n')
                        .count();
                    format!(" at line {line}")
                }
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

/// Doctor-only: when enabled (in doctor's worker process, never in the real
/// bar), every country flag generated by [`country_flag_from_iso_code`] is
/// recorded so the live block table can attribute the glyph to its block.
static FLAG_RECORDER: std::sync::OnceLock<std::sync::Mutex<Vec<String>>> =
    std::sync::OnceLock::new();

pub(crate) fn enable_flag_recorder() {
    let _ = FLAG_RECORDER.set(std::sync::Mutex::new(Vec::new()));
}

pub(crate) fn recorded_flags() -> Vec<String> {
    let mut flags = FLAG_RECORDER
        .get()
        .and_then(|m| m.lock().ok().map(|v| v.clone()))
        .unwrap_or_default();
    flags.sort_unstable();
    flags.dedup();
    flags
}

fn record_flag(flag: &str) {
    if let Some(recorder) = FLAG_RECORDER.get()
        && let Ok(mut recorder) = recorder.lock()
    {
        recorder.push(flag.to_string());
    }
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
    let flag = String::from_utf8(vec![0xf0, 0x9f, 0x87, b1, 0xf0, 0x9f, 0x87, b2]).unwrap();
    record_flag(&flag);
    flag
}

#[inline]
pub fn celsius_to_fahrenheit(c: f64) -> f64 {
    c * (9.0 / 5.0) + 32.0
}

#[inline]
pub fn fahrenheit_to_celsius(f: f64) -> f64 {
    (f - 32.0) * (5.0 / 9.0)
}

#[inline]
pub fn mps_to_kmh(mps: f64) -> f64 {
    mps * 3.6
}

#[inline]
pub fn kmh_to_mps(kmh: f64) -> f64 {
    kmh / 3.6
}

const KM_PER_MILE: f64 = 1.609344;

#[inline]
pub fn kmh_to_mph(kmh: f64) -> f64 {
    kmh / KM_PER_MILE
}

#[inline]
pub fn mph_to_kmh(mph: f64) -> f64 {
    mph * KM_PER_MILE
}

/// A shortcut for `Default::default()`
/// See <https://github.com/rust-lang/rust/issues/73014>
#[inline]
pub fn default<T: Default>() -> T {
    Default::default()
}

pub trait StreamExtDebounced: futures::StreamExt {
    fn next_debounced(&mut self) -> impl Future<Output = Option<Self::Item>>;
}

impl<T: futures::StreamExt + Unpin> StreamExtDebounced for T {
    async fn next_debounced(&mut self) -> Option<Self::Item> {
        let mut result = self.next().await?;
        let mut noop_ctx = std::task::Context::from_waker(std::task::Waker::noop());
        loop {
            match self.poll_next_unpin(&mut noop_ctx) {
                std::task::Poll::Ready(Some(x)) => result = x,
                std::task::Poll::Ready(None) | std::task::Poll::Pending => return Some(result),
            }
        }
    }
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
        assert!(
            !has_command("thequickbrownfoxjumpsoverthelazydog")
                .await
                .unwrap()
        );
    }

    #[test]
    fn test_flags() {
        assert!(country_flag_from_iso_code("ES") == "🇪🇸");
        assert!(country_flag_from_iso_code("US") == "🇺🇸");
        assert!(country_flag_from_iso_code("USA") == "USA");
    }

    #[test]
    fn test_file_candidates_absolute() {
        // extension is probed only when the path has none
        assert_eq!(
            file_candidates("/foo/awesome5", Some("icons"), Some("toml")),
            [
                PathBuf::from("/foo/awesome5"),
                PathBuf::from("/foo/awesome5.toml")
            ]
        );
        assert_eq!(
            file_candidates("/foo/awesome5.toml", Some("icons"), Some("toml")),
            [PathBuf::from("/foo/awesome5.toml")]
        );
    }

    #[test]
    fn test_file_candidates_relative() {
        let candidates = file_candidates("awesome5", Some("icons"), Some("toml"));
        // every candidate ends with the subdir + file, checked in pairs of
        // (no extension, with extension), with /usr/share always last
        assert!(candidates.len() >= 2);
        assert!(candidates.len().is_multiple_of(2));
        for pair in candidates.chunks(2) {
            assert!(
                pair[0].ends_with("i3status-rust/icons/awesome5")
                    || pair[0].ends_with("icons/awesome5")
            );
            assert_eq!(pair[1], pair[0].with_extension("toml"));
        }
        assert_eq!(
            candidates[candidates.len() - 2],
            PathBuf::from("/usr/share/i3status-rust/icons/awesome5")
        );

        // a name that already has an extension is not probed twice
        let candidates = file_candidates("awesome5.toml", Some("icons"), Some("toml"));
        assert!(candidates.iter().all(|c| c.extension().is_some()));
    }
}

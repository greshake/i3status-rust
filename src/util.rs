use crate::blocks::Block;
use crate::config::Config;
use crate::errors::*;
use serde::de::DeserializeOwned;
use serde::ser::Serialize;
use serde::Serializer;
use serde_json::value::Value;
use std::collections::HashMap;
use std::fmt::{Debug, Display, Formatter};
use std::fs::{File, OpenOptions};
use std::io::prelude::*;
use std::io::BufReader;
use std::num::ParseIntError;
use std::path::{Path, PathBuf};
use strfmt::FmtError;
use toml;

pub fn xdg_config_home() -> PathBuf {
    // In the unlikely event that $HOME is not set, it doesn't really matter
    // what we fall back on, so use /.config.
    let config_path = std::env::var("XDG_CONFIG_HOME").unwrap_or(format!(
        "{}/.config",
        std::env::var("HOME").unwrap_or("".to_string())
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
            "background": if sep_bg.is_some() { Value::String(sep_bg.unwrap()) } else { Value::Null },
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

pub fn color_from_rgba(color: &str) -> ::std::result::Result<(u8, u8, u8, u8), ParseIntError> {
    Ok((
        u8::from_str_radix(&color[1..3], 16)?,
        u8::from_str_radix(&color[3..5], 16)?,
        u8::from_str_radix(&color[5..7], 16)?,
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
pub fn add_colors(a: &str, b: &str) -> ::std::result::Result<String, ParseIntError> {
    let (r_a, g_a, b_a, a_a) = color_from_rgba(a)?;
    let (r_b, g_b, b_b, a_b) = color_from_rgba(b)?;

    Ok(color_to_rgba((
        r_a.checked_add(r_b).unwrap_or(255),
        g_a.checked_add(g_b).unwrap_or(255),
        b_a.checked_add(b_b).unwrap_or(255),
        a_a.checked_add(a_b).unwrap_or(255),
    )))
}

pub struct FormatTemplate {
    format_str: String,
}

impl Debug for FormatTemplate {
    fn fmt(&self, f: &mut Formatter) -> std::result::Result<(), std::fmt::Error> {
        let _ = write!(f, "FormatTemplate(\"{}\")", self.format_str);
        Ok(())
    }
}

impl FormatTemplate {
    pub fn from_string(s: &str) -> Result<FormatTemplate> {
        Ok(FormatTemplate {
            format_str: s.to_owned(),
        })
    }

    pub fn render<T: Serialize>(&self, vars: &T) -> String {
        fn try_render<T: Serialize>(
            format_str: &str,
            vars: &T,
        ) -> ::std::result::Result<String, ()> {
            if let Value::Object(ast) = serde_json::value::to_value(vars).map_err(|_| ())? {
                let formatter = |mut fmt: ::strfmt::Formatter| {
                    let v = match ast.get(fmt.key) {
                        Some(v) => v,
                        None => {
                            return Err(FmtError::KeyError(format!("Invalid key: {}", fmt.key)));
                        }
                    };

                    match v {
                        Value::String(s) => fmt.str(s),
                        Value::Number(n) => {
                            if let Some(n) = n.as_f64() {
                                fmt.f64(n)
                            } else if let Some(n) = n.as_u64() {
                                fmt.u64(n)
                            } else if let Some(n) = n.as_i64() {
                                fmt.i64(n)
                            } else {
                                Err(::strfmt::FmtError::TypeError(
                                    "unknown Number type".to_owned(),
                                ))
                            }
                        }
                        _ => Err(::strfmt::FmtError::TypeError("unknown type".to_owned())),
                    }
                };
                return ::strfmt::strfmt_map(format_str, &formatter).map_err(|_| ());
            }
            ::std::result::Result::Err(())
        };
        try_render(&self.format_str, vars).unwrap_or_else(|_| "\u{f321}".to_owned())
    }
}

pub struct DisplayableOption<T: Display, F: Display> {
    inner: Option<T>,
    fallback: F,
}

impl<T: Display, F: Display> Serialize for DisplayableOption<T, F> {
    fn serialize<S>(&self, serializer: S) -> ::std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if let Some(inner) = &self.inner {
            serializer.serialize_str(&format!("{}", inner))
        } else {
            serializer.serialize_str(&format!("{}", self.fallback))
        }
    }
}

impl<T: Display, F: Display> DisplayableOption<T, F> {
    pub fn new(value: Option<T>, fallback: F) -> Self {
        DisplayableOption {
            inner: value,
            fallback,
        }
    }
}

impl<T: Display, F: Display> Display for DisplayableOption<T, F> {
    fn fmt(&self, f: &mut Formatter) -> std::result::Result<(), std::fmt::Error> {
        match &self.inner {
            Some(value) => value.fmt(f),
            None => self.fallback.fmt(f),
        }
    }
}

macro_rules! if_debug {
    ($x:block) => (if cfg!(debug_assertions) $x)
}

macro_rules! mapped_struct {
    ($( #[$attr:meta] )* pub struct $name:ident : $fieldtype:ty { $( pub $fname:ident ),* }) => {
        $( #[$attr] )*
        pub struct $name {
            $( pub $fname : $fieldtype ),*
        }

        impl $name {
            #[allow(dead_code)]
            pub fn map(&self) -> ::std::collections::HashMap<&'static str, &$fieldtype> {
                let mut m = ::std::collections::HashMap::new();
                $( m.insert(stringify!($fname), &self.$fname); )*
                m
            }

            pub fn owned_map(&self) -> ::std::collections::HashMap<String, $fieldtype> {
                let mut m = ::std::collections::HashMap::new();
                $( m.insert(stringify!($fname).to_owned(), self.$fname.to_owned()); )*
                m
            }
        }
    }
}

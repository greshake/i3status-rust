use block::Block;
use config::Config;
use errors::*;
use std::collections::HashMap;
use serde::de::DeserializeOwned;
use serde_json::value::Value;
use toml;
use regex::Regex;
use std::prelude::v1::String;
use std::fmt::Display;
use std::fs::{File, OpenOptions};
use std::io::BufReader;
use std::io::prelude::*;
use std::num::ParseIntError;
use std::path::Path;

pub fn deserialize_file<T>(file: &str) -> Result<T>
where
    T: DeserializeOwned,
{
    let mut contents = String::new();
    let mut file = BufReader::new(File::open(file)
        .internal_error("util", "failed to open file")?);
    file.read_to_string(&mut contents)
        .internal_error("util", "failed to read file")?;
    toml::from_str(&contents).configuration_error("failed to parse TOML from file contents")
}

pub fn read_file(blockname: &str, path: &Path) -> Result<String> {
    let mut f = OpenOptions::new()
        .read(true)
        .open(path)
        .block_error(blockname, &format!("failed to open file {}", path.to_string_lossy()))?;
    let mut content = String::new();
    f.read_to_string(&mut content)
        .block_error(blockname, &format!("failed to read {}", path.to_string_lossy()))?;
    // Removes trailing newline
    content.pop();
    Ok(content)
}

#[allow(dead_code)]
pub fn get_file(name: &str) -> Result<String> {
    let mut file_contents = String::new();
    let mut file = File::open(name)
        .internal_error("util", &format!("Unable to open {}", name))?;
    file.read_to_string(&mut file_contents)
        .internal_error("util", &format!("Unable to read {}", name))?;
    Ok(file_contents)
}

macro_rules! match_range {// the `*` in `$(,)*` should be replaced with `?` if/when RFC 2298 lands on stable.
    ($a:expr, default: ($default:expr) {$($lower:expr ; $upper:expr => $e:expr),+} $(,)* ) => (
        match $a {
            $(
                t if t >= $lower && t <= $upper => { $e },
            )+
            _ => { $default }
        }
    )
}

macro_rules! map (// the `*` in `$(,)*` should be replaced with `?` if/when RFC 2298 lands on stable.
    { $($key:expr => $value:expr),+ $(,)* } => {
        {
            let mut m = ::std::collections::HashMap::new();
            $(
                m.insert($key, $value);
            )+
            m
        }
     };
);

macro_rules! map_to_owned (// the `*` in `$(,)*` should be replaced with `?` if/when RFC 2298 lands on stable.
    { $($key:expr => $value:expr),+ $(,)* } => {
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

pub fn print_blocks(order: &Vec<String>, block_map: &HashMap<String, &mut Block>, config: &Config) -> Result<()> {
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
        if widgets.len() == 0 {
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
                    "color": sep_fg
                });
        print!("{}{},", if state.has_predecessor { "," } else { "" },
               separator.to_string());
        print!("{}", first.to_string());
        state.set_last_bg(color.to_owned());
        state.set_predecessor(true);

        for widget in widgets.iter().skip(1) {
            print!("{}{}", if state.has_predecessor { "," } else { "" },
                   widget.to_string());
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
    format!("#{:02X}{:02X}{:02X}{:02X}", color.0, color.1, color.2, color.3)
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

#[derive(Debug, Clone)]
pub enum FormatTemplate {
    Str(String, Option<Box<FormatTemplate>>),
    Var(String, Option<Box<FormatTemplate>>),
}

impl FormatTemplate {
    pub fn from_string(s: String) -> Result<FormatTemplate> {
        let s_as_bytes = s.clone().into_bytes();

        //valid var tokens: {} containing any amount of alphanumericals
        let re = Regex::new(r"\{[a-zA-Z0-9]+?\}")
            .internal_error("util", "invalid regex")?;

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
                rendered.push_str(
                    &format!("{}", vars.get(key).expect(&format!("Unknown placeholder in format string: {}", key))),
                );
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
                rendered.push_str(&format!("{}",
                                           vars.get(&**key)
                                               .internal_error("util", &format!("Unknown placeholder in format string: {}", key))?));
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

macro_rules! mapped_struct {// the `*` in `$(,)*` should be replaced with `?` if/when RFC 2298 lands on stable.
    ($( #[$attr:meta] )* pub struct $name:ident : $fieldtype:ty { $( pub $fname:ident ),* $(,)* }) => {
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

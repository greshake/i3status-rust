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
use std::fs::File;
use std::io::BufReader;
use std::io::prelude::*;

pub fn deserialize_file<T>(file: &str) -> Result<T>
where
    T: DeserializeOwned
{
    let mut contents = String::new();
    let mut file = BufReader::new(File::open(file).internal_error("util", "failed to open file")?);
    file.read_to_string(&mut contents).internal_error("util", "failed to read file")?;
    toml::from_str(&contents).configuration_error("failed to parse TOML from file contents")
}

pub fn get_file(name: &str) -> Result<String> {
    let mut file_contents = String::new();
    let mut file = File::open(name)
        .internal_error("util", &format!("Unable to open {}", name))?;
    file.read_to_string(&mut file_contents)
        .internal_error("util", &format!("Unable to read {}", name))?;
    Ok(file_contents)
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
    pub has_predecessor: bool
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
        let ref block = *(block_map.get(block_id).internal_error("util", "couldn't get block by id")?);
        let widgets = block.view();
        let first = widgets[0];
        let color = first.get_rendered()["background"].as_str().internal_error("util", "couldn't get background color")?;

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
            state.set_last_bg(String::from(widget.get_rendered()["background"].as_str().internal_error("util", "couldn't get background color")?));
            state.set_predecessor(true);
        }
    }
    println!("],");

    Ok(())
}



#[derive(Debug,Clone)]
pub enum FormatTemplate {
    Str(String, Option<Box<FormatTemplate>>),
    Var(String, Option<Box<FormatTemplate>>),
}


impl FormatTemplate {
    pub fn from_string(s: String) -> Result<FormatTemplate> {
        let s_as_bytes = s.clone().into_bytes();

        //valid var tokens: {} containing any amount of alphanumericals
        let re = Regex::new(r"\{[a-zA-Z0-9]+?\}").internal_error("util", "invalid regex")?;

        let mut token_vec: Vec<FormatTemplate> =vec![];
        let mut start: usize = 0;

        for re_match in re.find_iter(&s) {
            if re_match.start() != start {
                let str_vec : Vec<u8> =(&s_as_bytes)[start..re_match.start()].to_vec();
                token_vec.push(FormatTemplate::Str(String::from_utf8(str_vec).internal_error("util", "failed to convert string from UTF8")?, None));
            }
            token_vec.push(FormatTemplate::Var(re_match.as_str().to_string(), None));
            start = re_match.end();
        }
        let str_vec : Vec<u8> = (&s_as_bytes)[start..].to_vec();
        token_vec.push(FormatTemplate::Str(String::from_utf8(str_vec).internal_error("util", "failed to convert string from UTF8")?,None));
        let mut template: FormatTemplate = match token_vec.pop() {
            Some(token) => {token},
            _ => FormatTemplate::Str("".to_string(), None)
        };
        while let Some(token) = token_vec.pop() {
            template = match token {
                FormatTemplate::Str(s, _) => FormatTemplate::Str(s, Some(Box::new(template))),
                FormatTemplate::Var(s, _) => FormatTemplate::Var(s, Some(Box::new(template)))
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
                rendered.push_str(&s);
                if let Some(ref next) = *next {
                    rendered.push_str(&*next.render(vars));
                };
            }
            Var(ref key, ref next) => {
                rendered.push_str(&format!("{}", &vars.get(key).expect(&format!("Unknown placeholder in format string: {}", key))));
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
                rendered.push_str(&s);
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

// any uses should be replaced with eprintln! once it is on stable
macro_rules! eprintln {
    ($fmt:expr, $($arg:tt)*) => {
        use ::std::io::Write;
        writeln!(&mut ::std::io::stderr(), $fmt, $($arg)*).ok();
    };
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

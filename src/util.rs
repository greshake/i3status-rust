use block::Block;
use std::collections::HashMap;
use serde_json::Value;
use regex::Regex;
use std::prelude::v1::String;
use std;
use std::fmt::Display;


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

struct PrintState {
    pub last_bg: Value,
    pub has_predecessor: bool
}

impl PrintState {
    fn set_last_bg(&mut self, bg: Value) {
        self.last_bg = bg;
    }
    fn set_predecessor(&mut self, pre: bool) {
        self.has_predecessor = pre;
    }
}

pub fn print_blocks(order: &Vec<String>, block_map: &HashMap<String, &mut Block>, theme: &Value) {
    let mut state = PrintState {
        has_predecessor: false,
        last_bg: Value::Null
    };

    print!("[");
    for block_id in order {
        let ref block = *(block_map.get(block_id).unwrap());
        let widgets = block.view();
        let first = widgets[0];
        let color = String::from(first.get_rendered()["background"].as_str().unwrap());

        // TODO: clean this up
        let tmp: Value = theme.get("separator_fg").expect("separator_fg entry is missing").clone();
        let sep_fg:Value= if tmp.as_str().unwrap() == "auto".to_string() {Value::String(color.clone())} else {tmp};

        let tmp = theme.get("separator_bg").expect("separator_bg entry is missing").clone();
        let sep_bg = if tmp.as_str().unwrap() == "auto".to_string() {state.last_bg.clone()} else {tmp};

        let separator = json!({
                    "full_text": theme["separator"],
                    "separator": false,
                    "separator_block_width": 0,
                    "background": sep_bg,
                    "color": sep_fg
                });
        print!("{}{},", if state.has_predecessor { "," } else { "" },
               separator.to_string());
        print!("{}", first.to_string());
        state.set_last_bg(Value::String(color));
        state.set_predecessor(true);

        for widget in widgets.iter().skip(1) {
            print!("{}{}", if state.has_predecessor { "," } else { "" },
                   widget.to_string());
            state.set_last_bg(Value::String(String::from(widget.get_rendered()["background"].as_str().unwrap())));
            state.set_predecessor(true);
        }
    }
    println!("],");
}



#[derive(Debug,Clone)]
pub enum FormatTemplate {
    Str(String, Option<Box<FormatTemplate>>),
    Var(String, Option<Box<FormatTemplate>>),
}


impl FormatTemplate {
    pub fn from_string(s: String) -> Result<FormatTemplate,std::string::FromUtf8Error> {
        let s_as_bytes = s.clone().into_bytes();

        //valid var tokens: {} containing any amount of alphanumericals
        let re = Regex::new(r"\{[a-zA-Z0-9]+?\}").unwrap();

        let mut token_vec: Vec<FormatTemplate> =vec![];
        let mut start: usize = 0;

        for re_match in re.find_iter(&s) {
            if re_match.start() != start {
                let str_vec : Vec<u8> =(&s_as_bytes)[start..re_match.start()].to_vec();
                token_vec.push(FormatTemplate::Str(String::from_utf8(str_vec)?, None));
            }
            token_vec.push(FormatTemplate::Var(re_match.as_str().to_string(), None));
            start = re_match.end();
        }
        let str_vec : Vec<u8> = (&s_as_bytes)[start..].to_vec();
        token_vec.push(FormatTemplate::Str(String::from_utf8(str_vec)?,None));
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

    pub fn render_static_str<T: Display>(&self, vars: &HashMap<&str, T>) -> String {
        use self::FormatTemplate::*;
        let mut rendered = String::new();
        match *self {
            Str(ref s, ref next) => {
                rendered.push_str(&s);
                if let Some(ref next) = *next {
                    rendered.push_str(&*next.render_static_str(vars));
                };
            }
            Var(ref key, ref next) => {
                rendered.push_str(&format!("{}", vars.get(&**key).expect(&format!("Unknown placeholder in format string: {}", key))));
                if let Some(ref next) = *next {
                    rendered.push_str(&*next.render_static_str(vars));
                };
            }
        };
        rendered
    }
}

macro_rules! get_str {
    ($config:expr, $name:expr) => {String::from($config[$name].as_str().expect(&format!("Required argument {} not found in block config!", $name)))};
}
macro_rules! get_str_default {
    ($config:expr, $name:expr, $default:expr) => {String::from($config[$name].as_str().unwrap_or($default))};
}

macro_rules! get_u64 {
    ($config:expr, $name:expr) => {$config[$name].as_u64().expect(&format!("Required argument {} not found in block config!", $name))};
}
macro_rules! get_u64_default {
    ($config:expr, $name:expr, $default:expr) => {$config[$name].as_u64().unwrap_or($default)};
}

macro_rules! get_f64 {
    ($config:expr, $name:expr) => {$config[$name].as_f64().expect(&format!("Required argument {} not found in block config!", $name))};
}
macro_rules! get_f64_default {
    ($config:expr, $name:expr, $default:expr) => {$config[$name].as_f64().unwrap_or($default)};
}



macro_rules! get_bool {
    ($config:expr, $name:expr) => {$config[$name].as_bool().expect(&format!("Required argument {} not found in block config!", $name))};
}
macro_rules! get_bool_default {
    ($config:expr, $name:expr, $default:expr) => {$config[$name].as_bool().unwrap_or($default)};
}

macro_rules! if_debug {
    ($x:block) => (if cfg!(debug_assertions) $x)
}

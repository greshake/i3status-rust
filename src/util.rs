use block::Block;
use std::collections::HashMap;
use serde_json::Value;

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

pub fn print_blocks(order: &Vec<String>, block_map: &HashMap<String, &mut Block>) {
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
        let s = json!({
                    "full_text": "",
                    "separator": false,
                    "separator_block_width": 0,
                    "background": state.last_bg.clone(),
                    "color": color.clone()
                });
        print!("{}{},",if state.has_predecessor {","} else {""},
               s.to_string());
        print!("{}", first.to_string());
        state.set_last_bg(Value::String(color));
        state.set_predecessor(true);

        for widget in widgets.iter().skip(1) {
            print!("{}{}",if state.has_predecessor {","} else {""},
                   widget.to_string());
            state.set_last_bg(Value::String(String::from(widget.get_rendered()["background"].as_str().unwrap())));
            state.set_predecessor(true);
        }
    }
    println!("],");
}

pub enum FormatTemplate {
    Str(String, Option<Box<FormatTemplate>>),
    Var(String, Option<Box<FormatTemplate>>),
    End
}

impl FormatTemplate {
    fn from_string(s: String) -> FormatTemplate {
        unimplemented!()
    }

    fn render(&self, vars: &HashMap<String, String>) -> String {
        use self::FormatTemplate::*;
        let mut rendered = String::new();
        match *self {
            Str(ref s, ref next) => {
                rendered.push_str(&s);
                if let Some(ref next) = *next {
                    rendered.push_str(&*next.render(vars));
                };
            },
            Var(ref key, ref next) => {
                rendered.push_str(&vars.get(key).expect(&format!("Unknown placeholder in format string: {}", key)));
                if let Some(ref next) = *next {
                    rendered.push_str(&*next.render(vars));
                };
            },
            _ => ()
        };
        rendered
    }
}

macro_rules! get_str{
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

macro_rules! get_bool{
    ($config:expr, $name:expr) => {$config[$name].as_bool().expect(&format!("Required argument {} not found in block config!", $name))};
}
macro_rules! get_bool_default{
    ($config:expr, $name:expr, $default:expr) => {$config[$name].as_bool().unwrap_or($default)};
}
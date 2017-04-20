use serde_json::Value;

#[derive(Debug, Copy, Clone)]
pub enum State {
    Idle,
    Info,
    Good,
    Warning,
    Critical
}

impl State {
    pub fn theme_keys(self) -> (&'static str, &'static str) {
        use self::State::*;
        match self {
            Idle => ("idle_bg", "idle_fg"),
            Info => ("info_bg", "info_fg"),
            Good => ("good_bg", "good_fg"),
            Warning => ("warning_bg", "warning_fg"),
            Critical => ("critical_bg", "critical_fg"),
        }
    }
}

pub trait Widget {
    fn to_string(&self) -> String;
    fn get_rendered(&self) -> &Value;
}

pub enum UIElement {
    Block(Vec<Box<UIElement>>),
    Widget(Box<Widget>),
    WidgetWithSeparator(Box<Widget>)
}

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

impl UIElement {
    pub fn print_elements(&self) {
        print!("[");
        self.print(PrintState {
            has_predecessor: false,
            last_bg: Value::Null
        });
        println!("],");
    }

    fn print(&self, state: PrintState) -> PrintState {
        use self::UIElement::*;
        let mut state = state;
        match *self {
            Block(ref elements) => {
                for element in (*elements).iter() {
                    state = element.print(state);
                }
            },
            Widget(ref w) => {
                print!("{}{}",if state.has_predecessor {","} else {""},
                       w.to_string());
                state.set_last_bg(Value::String(String::from(w.get_rendered()["background"].as_str().unwrap())));
                state.set_predecessor(true);
            },
            WidgetWithSeparator(ref w) => {
                let color = String::from(w.get_rendered()["background"].as_str().unwrap());
                let s = json!({
                    "full_text": "",
                    "separator": false,
                    "separator_block_width": 0,
                    "background": state.last_bg.clone(),
                    "color": color.clone()
                });
                print!("{}{},",if state.has_predecessor {","} else {""},
                       s.to_string());
                print!("{}", w.to_string());
                state.set_last_bg(Value::String(color));
                state.set_predecessor(true);
            }
        }
        state
    }
}
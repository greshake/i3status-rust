use block::{Block, MouseButton, Theme, Color};
use std::time::Duration;
use std::collections::HashMap;
use std::cell::Cell;

pub struct Toggle {
    pub state: Cell<bool>,
    pub name: &'static str,
}

impl Toggle {
    pub fn new(name: &'static str) -> Toggle {
        Toggle {
            state: Cell::new(true),
            name: name,
        }
    }
}


impl Block for Toggle {
    fn id(&self) -> Option<&str> {
        Some(self.name)
    }

    fn click(&self, button: MouseButton) {
        let s = self.state.get();
        self.state.set(!s);
    }

    fn get_status(&self, theme: &Theme) -> HashMap<&str, String> {
        map!{
            "full_text" => String::from("I can change color! Click me"),
            "color"     => {if self.state.get() { Color(0,0,0).to_string() }
            else { Color(255, 0, 0).to_string() }}
        }
    }
}

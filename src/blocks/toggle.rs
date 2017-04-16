use block::{Block, MouseButton, State};
use std::time::Duration;
use std::collections::HashMap;
use std::cell::Cell;
use serde_json::Value;

pub struct Toggle {
    pub state: Cell<State>,
    pub name: &'static str,
}

impl Toggle {
    pub fn new(name: &'static str) -> Toggle {
        Toggle {
            state: Cell::new(State::Idle),
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
        use self::State::*;
        self.state.set(match s {
            Idle => Info,
            Info => Good,
            Good => Warning,
            Warning => Critical,
            Critical => Idle
        });
    }

    fn get_status(&self, theme: &Value) -> Value {
        json!({
            "full_text": String::from("I can cycle state! Click me"),
        })
    }

    fn get_state(&self) -> State {
        self.state.get()
    }
}

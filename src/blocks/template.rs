use std::cell::{Cell, RefCell};
use std::time::Duration;
use serde_json::Value;
use block::{Block, MouseButton, State};


pub struct Template {
    value: RefCell<String>,
    name: &'static str,
    click_count: Cell<u32>,
}

impl Template {
    pub fn new(name: &'static str) -> Template {
        Template {
                value: RefCell::new(String::from("Hello World!")),
                name: name,
                click_count: Cell::new(0),
            }
    }
}


impl Block for Template
{
    fn id(&self) -> Option<&str> {
        Some(self.name)
    }

    fn update(&self) -> Option<Duration> {
        // No need to update periodically, this Block only reacts to clicks.
        // Otherwise, return a Duration until the next update here
        None
    }

    fn get_status(&self, _: &Value) -> Value {
        json!({
            "full_text" : self.value.clone().into_inner()
        })
    }

    fn get_state(&self) -> State {
        // Use this function to determine the state of your block.
        // This influences the color of the block based on the theme
        match self.click_count.get() {
            0...10  => State::Good,
            10...20 => State::Warning,
            _       => State::Critical
        }
    }

    fn click(&self, button: MouseButton) {
        match button {
            MouseButton::Left => {
                let old = self.click_count.get();
                let new: u32 = old + 1;
                self.click_count.set(new);
                *self.value.borrow_mut() = format!("Click Count: {}", new);
            },
            MouseButton::Right => {
                let old = self.click_count.get();
                let new: u32 = if old > 0 {old - 1} else {0};
                self.click_count.set(new);
                *self.value.borrow_mut() = format!("Click Count: {}", new);
            }
            _ => {}
        }
    }
}
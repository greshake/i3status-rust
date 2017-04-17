use std::cell::{Cell, RefCell};
use std::time::Duration;

use block::{Block, MouseButton, State};

use serde_json::Value;

pub struct Template {
    name: String,
    update_interval: Duration,

    some_value: RefCell<String>,
    click_count: Cell<u32>,
}

impl Template {
    pub fn new(config: Value) -> Template {
        Template {
            name: String::from(config["name"].as_str().expect("The argument 'name' in the block config is required!")),
            update_interval: Duration::new(config["interval"].as_u64().unwrap_or(5), 0),

            some_value: RefCell::new(String::from(config["hello"].as_str().unwrap_or("Hello World!"))),
            click_count: Cell::new(0),
        }
    }
}


impl Block for Template
{
    fn id(&self) -> Option<&str> {
        Some(&self.name)
    }

    fn update(&self) -> Option<Duration> {
        // No need to update periodically, this Block only reacts to clicks.
        // Otherwise, return a Duration until the next update here
        Some(self.update_interval.clone())
    }

    fn get_status(&self, _: &Value) -> Value {
        json!({
            "full_text" : self.some_value.clone().into_inner()
        })
    }

    fn get_state(&self) -> State {
        // Use this function to determine the state of your block.
        // This influences the color of the block based on the theme
        match self.click_count.get() {
            0 ... 10 => State::Good,
            10 ... 20 => State::Warning,
            _ => State::Critical
        }
    }

    fn click(&self, button: MouseButton) {
        match button {
            MouseButton::Left => {
                let old = self.click_count.get();
                let new: u32 = old + 1;
                self.click_count.set(new);
                *self.some_value.borrow_mut() = format!("Click Count: {}", new);
            }
            MouseButton::Right => {
                let old = self.click_count.get();
                let new: u32 = if old > 0 { old - 1 } else { 0 };
                self.click_count.set(new);
                *self.some_value.borrow_mut() = format!("Click Count: {}", new);
            }
            _ => {}
        }
    }
}
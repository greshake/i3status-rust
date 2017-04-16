extern crate chrono;

use std::cell::RefCell;
use std::time::Duration;

use block::Block;

use self::chrono::offset::local::Local;
use serde_json::Value;

const FORMAT: &'static str = "%a %d/%m %R";

pub struct Time {
    time: RefCell<String>,
    name: &'static str
}

impl Time {
    pub fn new(name: &'static str) -> Time {
        Time {
            time: RefCell::new(String::from("")),
            name: name,
        }
    }
}


impl Block for Time {
    fn id(&self) -> Option<&str> {
        Some(self.name) // Just a demonstration at this point
    }

    fn update(&self) -> Option<Duration> {
        *self.time.borrow_mut() = format!("  {} ", Local::now().format(FORMAT));
        Some(Duration::new(60, 0))
    }

    fn get_status(&self, _: &Value) -> Value {
        json!({
            "full_text": self.time.clone().into_inner()
        })
    }
}

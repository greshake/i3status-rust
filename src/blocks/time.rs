extern crate chrono;

use std::cell::RefCell;
use std::time::Duration;

use block::Block;
use self::chrono::offset::local::Local;
use serde_json::Value;

const FORMAT: &'static str = "%a %d/%m %R";

pub struct Time {
    time: RefCell<String>,
    name: String
}

impl Time {
    pub fn new(config: Value) -> Time {
        Time {
            time: RefCell::new(String::from("")),
            name: config["name"].as_str().expect("Name of the Block must be a string!").to_string(),
        }
    }
}


impl Block for Time {
    fn id(&self) -> Option<&str> {
        Some(&self.name) // Just a demonstration at this point
    }

    fn update(&self) -> Option<Duration> {
        *self.time.borrow_mut() = format!(" {} ", Local::now().format(FORMAT));
        Some(Duration::new(60, 0))
    }

    fn get_status(&self, theme: &Value) -> Value {
        json!({
            "full_text": format!("{}{}", theme["icons"]["time"].as_str().unwrap(),
                                           self.time.clone().into_inner())
        })
    }
}

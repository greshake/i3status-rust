extern crate chrono;

use std::cell::RefCell;
use std::time::Duration;

use block::Block;
use self::chrono::offset::local::Local;
use serde_json::Value;
use uuid::Uuid;


pub struct Time {
    time: RefCell<String>,
    format: String,
    name: String,
}

impl Time {
    pub fn new(config: Value) -> Time {
        Time {
            format: get_str_default!(config, "format", "%a %d/%m %R"),
            time: RefCell::new(String::from("")),
            name: Uuid::new_v4().simple().to_string(),
        }
    }
}


impl Block for Time {
    fn id(&self) -> Option<&str> {
        Some(&self.name)
    }

    fn update(&self) -> Option<Duration> {
        *self.time.borrow_mut() = format!(" {} ", Local::now().format(&self.format));
        Some(Duration::new(60, 0))
    }

    fn get_status(&self, theme: &Value) -> Value {
        json!({
            "full_text": format!("{}{}", theme["icons"]["time"].as_str().unwrap(),
                                           self.time.clone().into_inner())
        })
    }
}

extern crate chrono;

use block::{Block, MouseButton, Theme};
use self::chrono::offset::local::Local;
use std::time::Duration;
use std::collections::HashMap;
use std::cell::RefCell;

const FORMAT: &'static str = "%a %F %T";

pub struct Time
{
    time: RefCell<String>,
    name: &'static str
}

impl Time
{
    pub fn new(name: &'static str) -> Time
    {
        Time
        {
            time: RefCell::new(String::from("")),
            name: name
        }
    }
}


impl Block for Time
{
    fn id(&self) -> Option<&str> {
        Some(self.name) // Just a demonstration at this point
    }

    fn update(&self) -> Option<Duration>
    {
        *self.time.borrow_mut() = format!("{}", Local::now().format(FORMAT));
        Some(Duration::new(1, 0))
    }

    fn get_status(&self, theme: &Theme) -> HashMap<&str, String>
    {
        map!{
            "full_text" => self.time.clone().into_inner(),
            "color"     => theme.bg.to_string()
        }
    }
}
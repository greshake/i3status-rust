use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::time::Duration;

use block::{Block, MouseButton, Theme};


pub struct Template
{
    value: RefCell<String>,
    name: &'static str,
    click_count: Cell<u32>,
}

impl Template
{
    pub fn new(name: &'static str) -> Template
    {
        Template
            {
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

    fn update(&self) -> Option<Duration>
    {
        Some(Duration::new(2, 0))
    }

    fn get_status(&self, theme: &Theme) -> HashMap<&str, String>
    {
        map! {
            "full_text" => self.value.clone().into_inner(),
            "color"     => theme.fg.to_string()
        }
    }

    fn click(&self, button: MouseButton) {
        match button {
            MouseButton::Left => {
                let old = self.click_count.get();
                let new: u32 = old + 1;
                self.click_count.set(new);
                *self.value.borrow_mut() = format!("Click Count: {}", new);
            }
            _ => {}
        }
    }
}
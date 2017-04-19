extern crate chrono;

use std::cell::RefCell;
use std::time::Duration;

use block::Block;
use self::chrono::offset::local::Local;
use widgets::text::TextWidget;
use widget::{UIElement, Widget};
use serde_json::Value;
use uuid::Uuid;


pub struct Time {
    time: RefCell<TextWidget>,
    update_interval: Duration,
    format: String
}

impl Time {
    pub fn new(config: Value, theme: &Value) -> Time {
        Time {
            format: get_str_default!(config, "format", "%a %d/%m %R"),
            time: RefCell::new(TextWidget::new(theme.clone()).with_text("").with_icon("time")),
            update_interval: Duration::new(get_u64_default!(config, "interval", 5), 0),
        }
    }
}


impl Block for Time {
    fn update(&self) -> Option<Duration> {
        (*self.time.borrow_mut()).set_text(format!("{}", Local::now().format(&self.format)));
        Some(self.update_interval.clone())
    }

    fn get_ui(&self) -> Box<UIElement> {
        ui_list!(self.time)
    }
}

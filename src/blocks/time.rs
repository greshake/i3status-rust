extern crate chrono;

use std::time::Duration;

use block::Block;
use self::chrono::offset::local::Local;
use widgets::text::TextWidget;
use widget::{I3BarWidget};
use serde_json::Value;
use uuid::Uuid;


pub struct Time {
    time: TextWidget,
    id: String,
    update_interval: Duration,
    format: String
}

impl Time {
    pub fn new(config: Value, theme: Value) -> Time {
        Time {
            id: Uuid::new_v4().simple().to_string(),
            format: get_str_default!(config, "format", "%a %d/%m %R"),
            time: TextWidget::new(theme).with_text("").with_icon("time"),
            update_interval: Duration::new(get_u64_default!(config, "interval", 5), 0),
        }
    }
}


impl Block for Time {
    fn update(&mut self) -> Option<Duration> {
        self.time.set_text(format!("{}", Local::now().format(&self.format)));
        Some(self.update_interval.clone())
    }

    fn view(&self) -> Vec<&I3BarWidget> {
        vec![&self.time]
    }

    fn id(&self) -> &str {
        &self.id
    }
}

use std::cell::{Cell, RefCell};
use std::time::Duration;
use std::sync::mpsc::Sender;

use block::Block;
use widgets::text::TextWidget;
use widget::{UIElement, State, Widget};
use input::I3barEvent;
use scheduler::UpdateRequest;

use serde_json::Value;
use uuid::Uuid;

pub struct Template {
    text: RefCell<TextWidget>,
    name: String,
    update_interval: Duration,
    tx_update_request: Sender<UpdateRequest>,
}

impl Template {
    pub fn new(config: Value, tx: Sender<UpdateRequest>, theme: &Value) -> Template {
        Template {
            name: Uuid::new_v4().simple().to_string(),
            update_interval: Duration::new(get_u64_default!(config, "interval", 5), 0),
            text: RefCell::new(TextWidget::new(theme.clone()).with_text("I'm a Template!")),
            tx_update_request: tx
        }
    }
}


impl Block for Template
{
    fn update(&self) -> Option<Duration> {
        Some(self.update_interval.clone())
    }
    fn get_ui(&self) -> Box<UIElement> {
        ui!(self.text)
    }
    fn click(&self, event: &I3barEvent) {}
    fn id(&self) -> Option<&str> {
        Some(&self.name)
    }
}
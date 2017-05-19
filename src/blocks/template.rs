use std::time::Duration;
use std::sync::mpsc::Sender;

use block::Block;
use widgets::text::TextWidget;
use widget::I3BarWidget;
use input::I3barEvent;
use std::collections::HashMap;
use std::fmt::Display;
use scheduler::Task;

use util::FormatTemplate;
use serde_json::Value;
use uuid::Uuid;


pub struct Template {
    text: TextWidget,
    id: String,
    update_interval: Duration,

    //useful, but optional
    #[allow(dead_code)]
    theme: Value,
    #[allow(dead_code)]
    tx_update_request: Sender<Task>,
}

impl Template {
    pub fn new(config: Value, tx: Sender<Task>, theme: Value) -> Template {
        {
            let text = TextWidget::new(theme.clone()).with_text("Template");
            return Template {
                id: Uuid::new_v4().simple().to_string(),
                update_interval: Duration::new(get_u64_default!(config, "interval", 5), 0),
                text: text,
                tx_update_request: tx,
                theme: theme,
            }
        }
        
    }
}


impl Block for Template
{
    fn update(&mut self) -> Option<Duration> {
        Some(self.update_interval.clone())
    }
    fn view(&self) -> Vec<&I3BarWidget> {
        vec![&self.text]
    }
    fn click(&mut self, _: &I3barEvent) {}
    fn id(&self) -> &str {
        &self.id
    }
}
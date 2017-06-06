use std::time::Duration;
use std::sync::mpsc::Sender;

use config::Config;
use block::Block;
use widgets::text::TextWidget;
use widget::I3BarWidget;
use input::I3BarEvent;
use scheduler::Task;

use serde_json::Value;
use uuid::Uuid;


pub struct Template {
    text: TextWidget,
    id: String,
    update_interval: Duration,

    //useful, but optional
    #[allow(dead_code)]
    config: Config,
    #[allow(dead_code)]
    tx_update_request: Sender<Task>,
}

impl Template {
    pub fn new(block_config: Value, config: Config, tx: Sender<Task>) -> Template {
        Template {
            id: Uuid::new_v4().simple().to_string(),
            update_interval: Duration::new(get_u64_default!(block_config, "interval", 5), 0),
            text: TextWidget::new(config.clone()).with_text("Template"),
            tx_update_request: tx,
            config: config,
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
    fn click(&mut self, _: &I3BarEvent) {}
    fn id(&self) -> &str {
        &self.id
    }
}

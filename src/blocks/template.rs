use std::cell::{Cell, RefCell};
use std::time::Duration;
use std::sync::Sender;

use block::{Block, State};
use input::I3barEvent;
use scheduler::UpdateRequest;

use serde_json::Value;
use uuid::Uuid;

pub struct Template {
    name: String,
    update_interval: Duration,
    tx_update_request: Sender<UpdateRequest>,
}

impl Template {
    pub fn new(config: Value, tx: Sender<UpdateRequest>) -> Template {
        Template {
            name: Uuid::new_v4().simple().to_string(),
            update_interval: Duration::new(get_u64_default!(config, "interval", 5), 0),
            tx_update_request: tx
        }
    }
}


impl Block for Template
{
    fn id(&self) -> Option<&str> {
        Some(&self.name)
    }

    fn update(&self) -> Option<Duration> {
        Some(self.update_interval.clone())
    }

    fn get_status(&self, theme: &Value) -> Value {
        json!({
            "full_text" : "Template"
        })
    }

    fn get_state(&self) -> State {
        State::Idle
    }

    fn click(&self, event: I3barEvent) {
        
    }
}
use block::{Block, MouseButton, Theme};
use std::time::Duration;
use serde_json::Value;
use std::collections::HashMap;

pub struct Separator {}

impl Block for Separator {
    fn get_status(&self, theme: &Theme) -> Value {
        json!({
            "full_text": "".to_string()
        })
    }
}

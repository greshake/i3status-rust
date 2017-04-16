use block::Block;
use serde_json::Value;

pub struct Separator {}

impl Block for Separator {
    fn get_status(&self, _: &Value) -> Value {
        json!({
            "full_text": "".to_string(),
            "background": null
        })
    }
}

use block::{Block, MouseButton, Theme};
use std::time::Duration;
use std::collections::HashMap;

pub struct Separator {}

impl Block for Separator {
    fn get_status(&self, theme: &Theme) -> HashMap<&str, String> {
        map!{
            "full_text" => "|".to_string()
        }
    }
}

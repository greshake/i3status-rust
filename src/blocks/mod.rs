pub mod cpu;
pub mod disk_info;
pub mod time;
pub mod toggle;

use self::time::*;

use super::block::Block;

extern crate serde_json;
use serde_json::Value;

macro_rules! boxed ( { $b:expr } => { Box::new($b) as Box<Block> }; );

pub fn create_block(name: &str, config: Value) -> Box<Block> {
    match name {
        "time" => boxed!(Time::new(config)),
        _ => {
            panic!("Not a registered block: {}", name);
        }
    }
}
pub mod cpu;
pub mod disk_info;
pub mod template;
pub mod time;
pub mod toggle;

use self::cpu::*;
use self::time::*;
use self::disk_info::*;
use self::toggle::*;
use self::template::*;

use super::block::Block;

extern crate serde_json;
use serde_json::Value;

macro_rules! boxed ( { $b:expr } => { Box::new($b) as Box<Block> }; );

pub fn create_block(name: &str, config: Value) -> Box<Block> {
    match name {
        "time" => boxed!(Time::new(config)),
        "template" => boxed!(Template::new(config)),
        _ => {
            panic!("Ńot a registered block: {}", name);
        }
    }
}
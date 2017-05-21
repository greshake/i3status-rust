mod time;
mod template;
mod load;
mod memory;
mod cpu;
mod music;
mod battery;
mod custom;
mod disk_info;
mod pacman;
mod toggle;

use self::time::*;
use self::template::*;
use self::music::*;
use self::cpu::*;
use self::load::*;
use self::memory::*;
use self::battery::*;
use self::custom::*;
use self::disk_info::*;
use self::pacman::*;
use self::toggle::*;

use super::block::Block;
use super::scheduler::Task;

extern crate serde_json;
extern crate dbus;

use serde_json::Value;
use std::sync::mpsc::Sender;

macro_rules! boxed ( { $b:expr } => { Box::new($b) as Box<Block> }; );

pub fn create_block(name: &str, config: Value, tx_update_request: Sender<Task>, theme: &Value) -> Box<Block> {
    match name {
        "time" => boxed!(Time::new(config, theme.clone())),
        "template" => boxed!(Template::new(config, tx_update_request, theme.clone())),
        "music" => boxed!(Music::new(config, tx_update_request, theme)),
        "load" => boxed!(Load::new(config, theme.clone())),
        "memory" => boxed!(Memory::new(config, tx_update_request, theme.clone())),
        "cpu" => boxed!(Cpu::new(config, theme.clone())),
        "pacman" => boxed!(Pacman::new(config, theme.clone())),
        "battery" => boxed!(Battery::new(config, theme.clone())),
        "custom" => boxed!(Custom::new(config, tx_update_request, theme.clone())),
        "disk_info" => boxed!(DiskInfo::new(config, theme.clone())),
        "toggle" => boxed!(Toggle::new(config, theme.clone())),
        _ => {
            panic!("Not a registered block: {}", name);
        }
    }
}

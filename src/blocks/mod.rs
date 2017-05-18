//mod rotatingtext;
//pub mod cpu;
//pub mod disk_info;
mod time;
mod template;
//mod memory;
//pub mod toggle;
//mod music;
//pub mod music_play_button;

use self::time::*;
use self::template::*;
//use self::music::*;
//use self::music::*;
//use self::music_play_button::*;

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
        //"music" => boxed!(Music::new(config, tx_update_request, theme)),
        _ => {
            panic!("Not a registered block: {}", name);
        }
    }
}
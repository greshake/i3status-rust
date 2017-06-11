mod time;
mod template;
mod load;
mod memory;
mod cpu;
mod music;
mod battery;
mod custom;
mod disk_space;
mod pacman;
mod temperature;
mod toggle;
mod sound;
mod focused_window;
mod xrandr;

use config::Config;
use self::time::*;
use self::template::*;
use self::music::*;
use self::cpu::*;
use self::load::*;
use self::memory::*;
use self::battery::*;
use self::custom::*;
use self::disk_space::*;
use self::pacman::*;
use self::sound::*;
use self::toggle::*;
use self::focused_window::*;
use self::temperature::*;
use self::xrandr::*;

use super::block::Block;
use super::scheduler::Task;

extern crate dbus;

use serde::de::Deserialize;
use std::sync::mpsc::Sender;
use toml::value::Value;

macro_rules! boxed ( { $b:expr } => { Box::new($b) as Box<Block> }; );

pub fn create_block(name: &str, block_config: Value, config: Config, tx_update_request: Sender<Task>) -> Box<Block> {
    match name {
        "time" => boxed!(Time::new(block_config, config)),
        "template" => boxed!(Template::new(block_config, config, tx_update_request)),
        "music" => boxed!(Music::new(block_config, config, tx_update_request)),
        "load" => boxed!(Load::new(block_config, config)),
        "memory" => boxed!(Memory::new(block_config, config, tx_update_request)),
        "cpu" => boxed!(Cpu::new(block_config, config)),
        "pacman" => boxed!(Pacman::new(block_config, config)),
        "battery" => boxed!(Battery::new(block_config, config)),
        "custom" => boxed!(Custom::new(block_config, config, tx_update_request)),
        "disk_space" => boxed!(DiskSpace::new(block_config, config)),
        "toggle" => boxed!(Toggle::new(block_config, config)),
        "sound" => boxed!(Sound::new(block_config, config)),
        "temperature" => boxed!(Temperature::new(block_config, config)),
        "focused_window" => boxed!(FocusedWindow::new(block_config, config, tx_update_request)),
        "xrandr" => boxed!(Xrandr::new(block_config, config)),
        _ => panic!("Not a registered block: {}", name),
    }
}

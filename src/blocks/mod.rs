mod backlight;
mod battery;
mod cpu;
mod custom;
mod disk_space;
mod focused_window;
mod load;
mod maildir;
mod memory;
mod music;
mod net;
mod nvidia_gpu;
mod pacman;
mod sound;
mod speedtest;
mod temperature;
mod template;
mod time;
mod toggle;
mod upower;
mod uptime;
mod weather;
mod xrandr;

use config::Config;

use self::backlight::*;
use self::battery::*;
use self::cpu::*;
use self::custom::*;
use self::disk_space::*;
use self::focused_window::*;
use self::load::*;
use self::maildir::*;
use self::memory::*;
use self::music::*;
use self::net::*;
use self::nvidia_gpu::*;
use self::pacman::*;
use self::sound::*;
use self::speedtest::*;
use self::temperature::*;
use self::template::*;
use self::time::*;
use self::toggle::*;
use self::upower::*;
use self::uptime::*;
use self::weather::*;
use self::xrandr::*;

use super::block::{Block, ConfigBlock};
use errors::*;
use super::scheduler::Task;

extern crate dbus;

use serde::de::Deserialize;
use chan::Sender;
use toml::value::Value;

macro_rules! block {
    ($block_type:ident, $block_config:expr, $config:expr, $tx_update_request:expr) => {{
        let block_config: <$block_type as ConfigBlock>::Config = <$block_type as ConfigBlock>::Config::deserialize($block_config)
            .configuration_error("failed to deserialize block config")?;
        Ok(Box::new($block_type::new(block_config, $config, $tx_update_request)?) as Box<Block>)
    }}
}

macro_rules! blocks {
    ( $name:ident, $block_config:ident, $config:ident, $tx_update_request:ident ; $( $block_name:expr => $block_type:ident ),+ ) => {
        match $name {
            $(
                $block_name => block!($block_type, $block_config, $config, $tx_update_request),
             )*
            _ => Err(BlockError($name.to_string(), "Unknown block!".to_string())),
        }
    }
}

pub fn create_block(name: &str, block_config: Value, config: Config, tx_update_request: Sender<Task>) -> Result<Box<Block>> {
    blocks!(name, block_config, config, tx_update_request;
            "backlight" => Backlight,
            "battery" => Battery,
            "cpu" => Cpu,
            "custom" => Custom,
            "disk_space" => DiskSpace,
            "focused_window" => FocusedWindow,
            "load" => Load,
            "maildir" => Maildir,
            "memory" => Memory,
            "music" => Music,
            "net" => Net,
            "nvidia_gpu" => NvidiaGpu,
            "pacman" => Pacman,
            "sound" => Sound,
            "speedtest" => SpeedTest,
            "temperature" => Temperature,
            "template" => Template,
            "time" => Time,
            "toggle" => Toggle,
            "upower" => Upower,
            "uptime" => Uptime,
            "weather" => Weather,
            "xrandr" => Xrandr
    )
}

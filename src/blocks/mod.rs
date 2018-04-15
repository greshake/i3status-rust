pub mod backlight;
pub mod battery;
mod cpu;
mod custom;
mod disk_space;
mod focused_window;
mod load;
pub mod maildir;
mod memory;
mod music;
mod net;
mod networkmanager;
pub mod nvidia_gpu;
mod pacman;
mod plugin;
mod sound;
mod speedtest;
mod temperature;
mod template;
mod time;
mod toggle;
mod uptime;
mod bluetooth;
pub mod ibus;

use crate::config::Config;
use self::time::*;
use self::template::*;
use self::music::*;
use self::cpu::*;
use self::load::*;
use self::memory::*;
mod weather;
mod xrandr;

use self::backlight::Backlight;
use self::battery::*;
use self::custom::*;
use self::disk_space::*;
use self::focused_window::*;
use self::maildir::*;
use self::net::*;
use self::networkmanager::*;
use self::nvidia_gpu::*;
use self::pacman::*;
use self::plugin::*;
use self::sound::*;
use self::speedtest::*;
use self::temperature::*;
use self::toggle::*;
use self::uptime::*;
use self::bluetooth::*;
use self::ibus::*;

use super::block::{Block, ConfigBlock};
use crate::errors::*;
use self::weather::*;
use self::xrandr::*;

use super::scheduler::Task;

extern crate dbus;

use chan::Sender;
use serde::de::Deserialize;
use toml::value::Value;

macro_rules! block {
    ($block_type:ident, $block_config:expr, $config:expr, $tx_update_request:expr) => {{
        let block_config: <$block_type as ConfigBlock>::Config = <$block_type as ConfigBlock>::Config::deserialize($block_config).configuration_error("failed to deserialize block config")?;
        Ok(Box::new($block_type::new(block_config, $config, $tx_update_request)?) as Box<Block>)
    }};
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
            "time" => Time,
            "template" => Template,
            "music" => Music,
            "load" => Load,
            "memory" => Memory,
            "cpu" => Cpu,
            "pacman" => Pacman,
            "battery" => Battery,
            "custom" => Custom,
            "disk_space" => DiskSpace,
            "toggle" => Toggle,
            "sound" => Sound,
            "speedtest" => SpeedTest,
            "temperature" => Temperature,
            "focused_window" => FocusedWindow,
            "xrandr" => Xrandr,
            "net" => Net,
            "backlight" => Backlight,
            "weather" => Weather,
            "uptime" => Uptime,
            "nvidia_gpu" => NvidiaGpu,
            "maildir" => Maildir,
            "networkmanager" => NetworkManager,
            "bluetooth" => Bluetooth,
            "ibus" => IBus,
            "plugin" => Plugin
    )
}

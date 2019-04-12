pub mod backlight;
pub mod battery;
pub mod bluetooth;
pub mod cpu;
pub mod custom;
pub mod disk_space;
pub mod docker;
pub mod focused_window;
pub mod ibus;
pub mod keyboard_layout;
pub mod load;
pub mod maildir;
pub mod memory;
pub mod music;
pub mod net;
pub mod networkmanager;
pub mod nvidia_gpu;
pub mod pacman;
pub mod sound;
pub mod speedtest;
pub mod temperature;
pub mod template;
pub mod time;
pub mod toggle;
pub mod uptime;
pub mod weather;
pub mod xrandr;

use self::backlight::*;
use self::battery::*;
use self::bluetooth::*;
use self::cpu::*;
use self::custom::*;
use self::disk_space::*;
use self::docker::*;
use self::focused_window::*;
use self::ibus::*;
use self::keyboard_layout::*;
use self::load::*;
use self::maildir::*;
use self::memory::*;
use self::music::*;
use self::net::*;
use self::networkmanager::*;
use self::nvidia_gpu::*;
use self::pacman::*;
use self::sound::*;
use self::speedtest::*;
use self::temperature::*;
use self::template::*;
use self::time::*;
use self::toggle::*;
use self::uptime::*;
use self::weather::*;
use self::xrandr::*;

use crossbeam_channel::Sender;
use serde::de::Deserialize;
use toml::value::Value;

use crate::block::{Block, ConfigBlock};
use crate::config::Config;
use crate::errors::*;
use crate::scheduler::Task;

macro_rules! block {
    ($block_type:ident, $block_config:expr, $config:expr, $update_request:expr) => {{
        let block_config: <$block_type as ConfigBlock>::Config =
            <$block_type as ConfigBlock>::Config::deserialize($block_config)
                .configuration_error("Failed to deserialize block config.")?;
        Ok(Box::new($block_type::new(block_config, $config, $update_request)?) as Box<Block>)
    }};
}

pub fn create_block(
    name: &str,
    block_config: Value,
    config: Config,
    update_request: Sender<Task>,
) -> Result<Box<Block>> {
    match name {
        // Please keep these in alphabetical order.
        "backlight" => block!(Backlight, block_config, config, update_request),
        "battery" => block!(Battery, block_config, config, update_request),
        "bluetooth" => block!(Bluetooth, block_config, config, update_request),
        "cpu" => block!(Cpu, block_config, config, update_request),
        "custom" => block!(Custom, block_config, config, update_request),
        "disk_space" => block!(DiskSpace, block_config, config, update_request),
        "docker" => block!(Docker, block_config, config, update_request),
        "focused_window" => block!(FocusedWindow, block_config, config, update_request),
        "ibus" => block!(IBus, block_config, config, update_request),
        "keyboard_layout" => block!(KeyboardLayout, block_config, config, update_request),
        "load" => block!(Load, block_config, config, update_request),
        "maildir" => block!(Maildir, block_config, config, update_request),
        "memory" => block!(Memory, block_config, config, update_request),
        "music" => block!(Music, block_config, config, update_request),
        "net" => block!(Net, block_config, config, update_request),
        "networkmanager" => block!(NetworkManager, block_config, config, update_request),
        "nvidia_gpu" => block!(NvidiaGpu, block_config, config, update_request),
        "pacman" => block!(Pacman, block_config, config, update_request),
        "sound" => block!(Sound, block_config, config, update_request),
        "speedtest" => block!(SpeedTest, block_config, config, update_request),
        "temperature" => block!(Temperature, block_config, config, update_request),
        "template" => block!(Template, block_config, config, update_request),
        "time" => block!(Time, block_config, config, update_request),
        "toggle" => block!(Toggle, block_config, config, update_request),
        "uptime" => block!(Uptime, block_config, config, update_request),
        "weather" => block!(Weather, block_config, config, update_request),
        "xrandr" => block!(Xrandr, block_config, config, update_request),
        other => Err(BlockError(other.to_string(), "Unknown block!".to_string())),
    }
}

pub mod apt;
pub mod backlight;
pub mod base_block;
pub mod battery;
pub mod bluetooth;
pub mod cpu;
pub mod custom;
pub mod custom_dbus;
pub mod disk_space;
pub mod docker;
pub mod focused_window;
pub mod github;
pub mod hueshift;
pub mod ibus;
pub mod kdeconnect;
pub mod keyboard_layout;
pub mod load;
pub mod maildir;
pub mod memory;
pub mod music;
pub mod net;
pub mod networkmanager;
pub mod notify;
#[cfg(feature = "notmuch")]
pub mod notmuch;
pub mod nvidia_gpu;
pub mod pacman;
pub mod pomodoro;
pub mod sound;
pub mod speedtest;
pub mod taskwarrior;
pub mod temperature;
pub mod template;
pub mod time;
pub mod toggle;
pub mod uptime;
pub mod watson;
pub mod weather;
pub mod xrandr;

use self::apt::*;
use self::backlight::*;
use self::base_block::*;
use self::battery::*;
use self::bluetooth::*;
use self::cpu::*;
use self::custom::*;
use self::custom_dbus::*;
use self::disk_space::*;
use self::docker::*;
use self::focused_window::*;
use self::github::*;
use self::hueshift::*;
use self::ibus::*;
use self::kdeconnect::*;
use self::keyboard_layout::*;
use self::load::*;
use self::maildir::*;
use self::memory::*;
use self::music::*;
use self::net::*;
use self::networkmanager::*;
use self::notify::*;
#[cfg(feature = "notmuch")]
use self::notmuch::*;
use self::nvidia_gpu::*;
use self::pacman::*;
use self::pomodoro::*;
use self::sound::*;
use self::speedtest::*;
use self::taskwarrior::*;
use self::temperature::*;
use self::template::*;
use self::time::*;
use self::toggle::*;
use self::uptime::*;
use self::watson::*;
use self::weather::*;
use self::xrandr::*;

use std::time::Duration;

use crossbeam_channel::Sender;
use serde::de::Deserialize;
use toml::value::Value;

use crate::config::Config;
use crate::errors::*;
use crate::input::I3BarEvent;
use crate::scheduler::Task;
use crate::widget::I3BarWidget;

#[derive(Clone, Debug, PartialEq)]
pub enum Update {
    Every(Duration),
    Once,
}

impl Default for Update {
    fn default() -> Self {
        Update::Once
    }
}

impl Into<Update> for Duration {
    fn into(self) -> Update {
        Update::Every(self)
    }
}

pub trait Block {
    /// A unique id for the block.
    fn id(&self) -> usize;

    /// The current "view" of the block, comprised of widgets.
    fn view(&self) -> Vec<&dyn I3BarWidget>;

    /// Forces an update of the internal state of the block.
    fn update(&mut self) -> Result<Option<Update>> {
        Ok(None)
    }

    ///Sends a signal event with the provided signal, this function is called on every block
    ///for every signal event
    fn signal(&mut self, _signal: i32) -> Result<()> {
        Ok(())
    }

    /// Sends click events to the block. This function is called on every block
    /// for every click; filter events by using the `event.name` property.
    fn click(&mut self, _event: &I3BarEvent) -> Result<()> {
        Ok(())
    }
}

pub trait ConfigBlock: Block {
    type Config;

    /// Creates a new block from the relevant configuration.
    fn new(
        id: usize,
        block_config: Self::Config,
        config: Config,
        update_request: Sender<Task>,
    ) -> Result<Self>
    where
        Self: Sized;

    fn override_on_click(&mut self) -> Option<&mut Option<String>> {
        None
    }
}

macro_rules! block {
    ($block_type:ident, $id:expr, $block_config:expr, $config:expr, $update_request:expr) => {{
        let common_config = BaseBlockConfig::extract(&mut $block_config);
        let mut common_config = BaseBlockConfig::deserialize(common_config)
            .configuration_error("Failed to deserialize common block config.")?;

        let block_config: <$block_type as ConfigBlock>::Config =
            <$block_type as ConfigBlock>::Config::deserialize($block_config)
                .configuration_error("Failed to deserialize block config.")?;

        let mut main_config = $config;
        if let Some(ref overrides) = block_config.color_overrides {
            for entry in overrides {
                match entry.0.as_str() {
                    "idle_fg" => main_config.theme.idle_fg = Some(entry.1.to_string()),
                    "idle_bg" => main_config.theme.idle_bg = Some(entry.1.to_string()),
                    "info_fg" => main_config.theme.info_fg = Some(entry.1.to_string()),
                    "info_bg" => main_config.theme.info_bg = Some(entry.1.to_string()),
                    "good_fg" => main_config.theme.good_fg = Some(entry.1.to_string()),
                    "good_bg" => main_config.theme.good_bg = Some(entry.1.to_string()),
                    "warning_fg" => main_config.theme.warning_fg = Some(entry.1.to_string()),
                    "warning_bg" => main_config.theme.warning_bg = Some(entry.1.to_string()),
                    "critical_fg" => main_config.theme.critical_fg = Some(entry.1.to_string()),
                    "critical_bg" => main_config.theme.critical_bg = Some(entry.1.to_string()),
                    // TODO the below as well?
                    // "separator"
                    // "separator_bg"
                    // "separator_fg"
                    // "alternating_tint_bg"
                    _ => (),
                }
            }
        }

        let mut block = $block_type::new($id, block_config, main_config, $update_request)?;
        if let Some(overrided) = block.override_on_click() {
            *overrided = common_config.on_click.take();
        }

        Ok(Box::new(BaseBlock {
            name: stringify!($block_type).to_string(),
            inner: block,
            on_click: common_config.on_click,
        }) as Box<dyn Block>)
    }};
}

pub fn create_block(
    id: usize,
    name: &str,
    mut block_config: Value,
    config: Config,
    update_request: Sender<Task>,
) -> Result<Box<dyn Block>> {
    match name {
        // Please keep these in alphabetical order.
        "apt" => block!(Apt, id, block_config, config, update_request),
        "backlight" => block!(Backlight, id, block_config, config, update_request),
        "battery" => block!(Battery, id, block_config, config, update_request),
        "bluetooth" => block!(Bluetooth, id, block_config, config, update_request),
        "cpu" => block!(Cpu, id, block_config, config, update_request),
        "custom" => block!(Custom, id, block_config, config, update_request),
        "custom_dbus" => block!(CustomDBus, id, block_config, config, update_request),
        "disk_space" => block!(DiskSpace, id, block_config, config, update_request),
        "docker" => block!(Docker, id, block_config, config, update_request),
        "focused_window" => block!(FocusedWindow, id, block_config, config, update_request),
        "github" => block!(Github, id, block_config, config, update_request),
        "hueshift" => block!(Hueshift, id, block_config, config, update_request),
        "ibus" => block!(IBus, id, block_config, config, update_request),
        "kdeconnect" => block!(KDEConnect, id, block_config, config, update_request),
        "keyboard_layout" => block!(KeyboardLayout, id, block_config, config, update_request),
        "load" => block!(Load, id, block_config, config, update_request),
        "maildir" => block!(Maildir, id, block_config, config, update_request),
        "memory" => block!(Memory, id, block_config, config, update_request),
        "music" => block!(Music, id, block_config, config, update_request),
        "net" => block!(Net, id, block_config, config, update_request),
        "networkmanager" => block!(NetworkManager, id, block_config, config, update_request),
        "notify" => block!(Notify, id, block_config, config, update_request),
        #[cfg(feature = "notmuch")]
        "notmuch" => block!(Notmuch, id, block_config, config, update_request),
        "nvidia_gpu" => block!(NvidiaGpu, id, block_config, config, update_request),
        "pacman" => block!(Pacman, id, block_config, config, update_request),
        "pomodoro" => block!(Pomodoro, id, block_config, config, update_request),
        "sound" => block!(Sound, id, block_config, config, update_request),
        "speedtest" => block!(SpeedTest, id, block_config, config, update_request),
        "taskwarrior" => block!(Taskwarrior, id, block_config, config, update_request),
        "temperature" => block!(Temperature, id, block_config, config, update_request),
        "template" => block!(Template, id, block_config, config, update_request),
        "time" => block!(Time, id, block_config, config, update_request),
        "toggle" => block!(Toggle, id, block_config, config, update_request),
        "uptime" => block!(Uptime, id, block_config, config, update_request),
        "watson" => block!(Watson, id, block_config, config, update_request),
        "weather" => block!(Weather, id, block_config, config, update_request),
        "xrandr" => block!(Xrandr, id, block_config, config, update_request),
        other => Err(BlockError(other.to_string(), "Unknown block!".to_string())),
    }
}

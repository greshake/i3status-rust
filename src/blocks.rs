pub mod apt;
pub mod backlight;
pub mod battery;
pub mod bluetooth;
pub mod cpu;
pub mod custom;
pub mod custom_dbus;
pub mod disk_space;
pub mod dnf;
pub mod docker;
pub mod external_ip;
pub mod focused_window;
pub mod github;
pub mod hueshift;
pub mod ibus;
pub mod kdeconnect;
pub mod keyboard_layout;
#[cfg(feature = "virt")]
pub mod libvirt;
pub mod load;
#[cfg(feature = "maildir")]
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
pub mod rofication;
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

mod base_block;
use base_block::*;

use self::apt::*;
use self::backlight::*;
use self::battery::*;
use self::bluetooth::*;
use self::cpu::*;
use self::custom::*;
use self::custom_dbus::*;
use self::disk_space::*;
use self::dnf::*;
use self::docker::*;
use self::external_ip::*;
use self::focused_window::*;
use self::github::*;
use self::hueshift::*;
use self::ibus::*;
use self::kdeconnect::*;
use self::keyboard_layout::*;
#[cfg(feature = "virt")]
use self::libvirt::Libvirt;
use self::load::*;
#[cfg(feature = "maildir")]
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
use self::rofication::*;
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

use std::process::Command;
use std::time::Duration;

use crossbeam_channel::Sender;
use serde::de::{self, Deserialize, DeserializeOwned};
use toml::value::Value;

use crate::config::SharedConfig;
use crate::errors::*;
use crate::protocol::i3bar_event::I3BarEvent;
use crate::scheduler::Task;
use crate::widgets::I3BarWidget;

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

impl From<Duration> for Update {
    fn from(d: Duration) -> Update {
        Update::Every(d)
    }
}

/// The ConfigBlock trait combines a constructor (new(...)) and an associated configuration type
/// to form a block that can be instantiated from a piece of TOML (from the block configuration).
/// The associated type has to be a deserializable struct, which you can then use to get your
/// configurations from. The template shows you how to instantiate a simple Text widget.
/// For more info on how to use widgets, just look into other Blocks. More documentation to come.
///
/// The sender object can be used to send asynchronous update request for any block from a separate
/// thread, provide you know the Block's ID. This advanced feature can be used to reduce
/// the number of system calls by asynchronously waiting for events. A usage example can be found
/// in the Music block, which updates only when dbus signals a new song.
pub trait ConfigBlock: Block {
    type Config: DeserializeOwned;

    /// Creates a new block from the relevant configuration.
    fn new(
        id: usize,
        block_config: Self::Config,
        shared_config: SharedConfig,
        update_request: Sender<Task>,
    ) -> Result<Self>
    where
        Self: Sized;

    /// TODO: write documentation
    fn override_on_click(&mut self) -> Option<&mut Option<String>> {
        None
    }
}

/// The Block trait is used to interact with a block after it has been instantiated from ConfigBlock
pub trait Block {
    /// Use this function to return the widgets that comprise the UI of your component.
    ///
    /// The music block may, for example, be comprised of a text widget and multiple
    /// buttons (buttons are also TextWidgets). Use a vec to wrap the references to your view.
    fn view(&self) -> Vec<&dyn I3BarWidget>;

    /// Required if you don't want a static block.
    ///
    /// Use this function to update the internal state of your block, for example during
    /// periodic updates. Return the duration until your block wants to be updated next.
    /// For example, a clock could request only to be updated every 60 seconds by returning
    /// Some(Update::Every(Duration::new(60, 0))) every time. If you return None,
    /// this function will not be called again automatically.
    fn update(&mut self) -> Result<Option<Update>> {
        Ok(None)
    }

    /// Sends a signal event with the provided signal, this function is called on every block
    /// for every signal event
    fn signal(&mut self, _signal: i32) -> Result<()> {
        Ok(())
    }

    /// Sends click events to the block.
    ///
    /// Here you can react to the user clicking your block. The I3BarEvent instance contains all
    /// fields to describe the click action, including mouse button and location down to the pixel.
    /// You may also update the internal state here.
    ///
    /// If block uses more that one widget, use the event.instance property to determine which widget was clicked.
    fn click(&mut self, _event: &I3BarEvent) -> Result<()> {
        Ok(())
    }
}

macro_rules! define_blocks {
    {
        $( $(#[cfg($attr: meta)])? $block: ident :: $block_type : ident $(,)? )*
    } => {
        $(
            $(#[cfg($attr)])?
            pub mod $block;
        )*

        #[derive(Debug, Clone, Copy)]
        pub enum BlockType {
            $(
                $(#[cfg($attr)])?
                #[allow(non_camel_case_types)]
                $block,
            )*
        }

        impl BlockType {
            pub fn create_block(
                self,
                id: usize,
                block_config: Value,
                shared_config: SharedConfig,
                update_request: Sender<Task>,
            ) -> Result<Option<Box<dyn Block>>>
            {
                match self {
                    $(
                        $(#[cfg($attr)])?
                        Self::$block => {
                            create_block_typed::<$block::$block_type>(id, block_config, shared_config, update_request)
                        }
                    )*
                }
            }
        }

        impl<'de> Deserialize<'de> for BlockType {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: de::Deserializer<'de>,
            {
                struct Visitor;

                impl<'de> de::Visitor<'de> for Visitor {
                    type Value = BlockType;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                        formatter.write_str("a block name")
                    }

                    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
                    where
                        E: de::Error,
                    {
                        match v {
                            $(
                            $(#[cfg($attr)])?
                            stringify!($block) => Ok(BlockType::$block),
                            $(
                            #[cfg(not($attr))]
                            stringify!($block) => Err(E::custom(format!("Block '{}' has to be enabled at the compile time", stringify!($block)))),
                            )?
                            )*
                            unknown => Err(E::custom(format!("Unknown block '{unknown}'")))
                        }
                    }
                }

                deserializer.deserialize_str(Visitor)
            }
        }

    };
}

// Please keep these in alphabetical order.
define_blocks!(
    apt::Apt,
    backlight::Backlight,
    battery::Battery,
    bluetooth::Bluetooth,
    cpu::Cpu,
    custom::Custom,
    custom_dbus::CustomDBus,
    disk_space::DiskSpace,
    dnf::Dnf,
    docker::Docker,
    external_ip::ExternalIP,
    focused_window::FocusedWindow,
    github::Github,
    hueshift::Hueshift,
    ibus::IBus,
    kdeconnect::KDEConnect,
    keyboard_layout::KeyboardLayout,
    load::Load,
    #[cfg(feature = "maildir")]
    maildir::Maildir,
    memory::Memory,
    music::Music,
    net::Net,
    networkmanager::NetworkManager,
    notify::Notify,
    #[cfg(feature = "notmuch")]
    notmuch::Notmuch,
    nvidia_gpu::NvidiaGpu,
    pacman::Pacman,
    pomodoro::Pomodoro,
    rofication::Rofication,
    sound::Sound,
    speedtest::SpeedTest,
    taskwarrior::Taskwarrior,
    temperature::Temperature,
    time::Time,
    toggle::Toggle,
    uptime::Uptime,
    watson::Watson,
    weather::Weather,
    xrandr::Xrandr,
);

pub fn create_block_typed<B>(
    id: usize,
    mut block_config: Value,
    mut shared_config: SharedConfig,
    update_request: Sender<Task>,

) -> Result<Option<Box<dyn Block>>>
where
    B: Block + ConfigBlock + 'static,
{
    // Extract base(common) config
    let common_config = BaseBlockConfig::extract(&mut block_config);
    let mut common_config = BaseBlockConfig::deserialize(common_config)
        .configuration_error("Failed to deserialize common block config.")?;

    // Run if_command if present
    if let Some(ref cmd) = common_config.if_command {
        if !Command::new("sh")
            .args(["-c", cmd])
            .output()?
            .status
            .success()
        {
            return Ok(None);
        }
    }

    // Apply theme overrides if presented
    if let Some(ref overrides) = common_config.theme_overrides {
        shared_config.theme_override(overrides)?;
    }
    if let Some(overrides) = common_config.icons_format {
        shared_config.icons_format_override(overrides);
    }
    if let Some(overrides) = common_config.icons_overrides {
        shared_config.icons_override(overrides);
    }

    // Extract block-specific config
    let block_config = <B as ConfigBlock>::Config::deserialize(block_config)
        .configuration_error("Failed to deserialize block config.")?;

    let mut block = B::new(id, block_config, shared_config, update_request)?;
    if let Some(overrided) = block.override_on_click() {
        *overrided = common_config.on_click.take();
    }

    Ok(Some(Box::new(BaseBlock {
        name: stringify!($block_type).to_string(),
        inner: block,
        on_click: common_config.on_click,
    })))
}

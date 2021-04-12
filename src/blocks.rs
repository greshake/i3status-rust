// pub mod apt;
// pub mod backlight;
pub mod base_block;
// pub mod battery;
// pub mod bluetooth;
pub mod cpu;
pub mod custom;
// pub mod custom_dbus;
// pub mod disk_space;
// pub mod docker;
// pub mod focused_window;
// pub mod github;
// pub mod hueshift;
// pub mod ibus;
// pub mod kdeconnect;
// pub mod keyboard_layout;
// pub mod load;
// #[cfg(feature = "maildir")]
// pub mod maildir;
// pub mod memory;
// pub mod music;
// pub mod net;
// pub mod networkmanager;
// pub mod notify;
// #[cfg(feature = "notmuch")]
// pub mod notmuch;
// pub mod nvidia_gpu;
// pub mod pacman;
// pub mod pomodoro;
// pub mod sound;
// pub mod speedtest;
// pub mod taskwarrior;
// pub mod temperature;
// pub mod template;
// pub mod time;
// pub mod toggle;
// pub mod uptime;
// pub mod watson;
// pub mod weather;
// pub mod xrandr;

use self::base_block::{BaseBlock, BaseBlockConfig};

use std::{cell::RefCell, pin::Pin};
use std::{rc::Rc, time::Duration};

use async_trait::async_trait;
use crossbeam_channel::Sender;
use futures::future::FutureExt;
use futures::stream::{self, Stream, StreamExt};
use serde::de::Deserialize;
use tokio::sync::mpsc;
use tokio_stream::wrappers::{IntervalStream, UnboundedReceiverStream};
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

impl Update {
    /// Return a stream that ticks according to what is specified.
    fn into_stream(self) -> Pin<Box<dyn Stream<Item = ()>>> {
        match self {
            Update::Every(dur) => {
                let interval = tokio::time::interval(dur);
                Box::pin(IntervalStream::new(interval).map(|_| ()))
            }
            Update::Once => Box::pin(async {}.into_stream()),
        }
    }
}

impl Default for Update {
    fn default() -> Self {
        Update::Once
    }
}

impl From<Duration> for Update {
    fn from(val: Duration) -> Self {
        Update::Every(val)
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
    type Config;

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
#[async_trait(?Send)]
pub trait Block {
    /// A unique id for the block (asigend by the constructor).
    fn id(&self) -> usize;

    /// Use this function to return the widgets that comprise the UI of your component.
    ///
    /// The music block may, for example, be comprised of a text widget and multiple
    /// buttons (buttons are also TextWidgets). Use a vec to wrap the references to your view.
    async fn render(&'_ mut self) -> Result<Vec<Box<dyn I3BarWidget>>>;

    /// Required if you don't want a static block.
    ///
    /// Use this function to update the internal state of your block, for example during
    /// periodic updates. Return the duration until your block wants to be updated next.
    /// For example, a clock could request only to be updated every 60 seconds by returning
    /// Some(Update::Every(Duration::new(60, 0))) every time. If you return None,
    /// this function will not be called again automatically.
    fn update_interval(&self) -> Update {
        Update::Once
    }

    /// Sends a signal event with the provided signal, this function is called on every block
    /// for every signal event
    async fn signal(&mut self, _signal: i32) -> Result<()> {
        Ok(())
    }

    /// Sends click events to the block.
    ///
    /// Here you can react to the user clicking your block. The I3BarEvent instance contains all
    /// fields to describe the click action, including mouse button and location down to the pixel.
    /// You may also update the internal state here.
    ///
    /// If block uses more that one widget, use the event.instance property to determine which widget was clicked.
    fn click(&mut self, _event: I3BarEvent) -> Result<()> {
        Ok(())
    }
}

#[async_trait(?Send)]
impl<T: Block + ?Sized> Block for Box<T> {
    fn id(&self) -> usize {
        self.as_ref().id()
    }

    async fn render(&'_ mut self) -> Result<Vec<Box<dyn I3BarWidget>>> {
        self.as_mut().render().await
    }

    fn update_interval(&self) -> Update {
        self.as_ref().update_interval()
    }

    async fn signal(&mut self, signal: i32) -> Result<()> {
        self.as_mut().signal(signal).await
    }

    fn click(&mut self, event: I3BarEvent) -> Result<()> {
        self.as_mut().click(event)
    }
}

pub enum Event {
    Update,
    Signal(i32),
    Clic(I3BarEvent),
}

#[allow(clippy::type_complexity)]
pub fn block_into_stream<'a>(
    block: impl Block + 'a,
) -> (
    impl Stream<Item = Vec<Box<dyn I3BarWidget>>> + 'a,
    mpsc::UnboundedSender<Event>,
) {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let update_events = Box::pin(block.update_interval().into_stream().map(|_| Event::Update));
    let events = stream::select(UnboundedReceiverStream::new(rx), update_events);

    // Moving block into a Rc<RefCell<...>> has no practical interest, it just
    // helps dealing with the borrow checker by moving some invariants checks
    // to runtime.
    //
    // Any improvement would be welcome but it may be hard here as Streams
    // could be executed in parallel, for exemple through `for_each_concurrent`.
    let block = Rc::new(RefCell::new(block));

    let stream = events.filter_map(move |event| {
        let block = block.clone();

        async move {
            let mut block_mut = block
                .try_borrow_mut()
                .expect("block update performed concurently");

            match event {
                Event::Update => Some(
                    block_mut
                        .render()
                        .await
                        .expect("failed during block rendering"),
                ),
                Event::Signal(signal) => {
                    block_mut
                        .signal(signal)
                        .await
                        .expect("failed during block update");
                    None
                }
                Event::Clic(event) => {
                    block_mut.click(event).expect("failed during block click");
                    None
                }
            }
        }
    });

    (stream, tx)
}

macro_rules! block {
    ($block_type:ident, $id:expr, $block_config:expr, $shared_config:expr, $update_request:expr) => {{
        // Extract base(common) config
        let common_config = BaseBlockConfig::extract(&mut $block_config);
        let mut common_config = BaseBlockConfig::deserialize(common_config)
            .configuration_error("Failed to deserialize common block config.")?;

        // Apply theme overrides if presented
        if let Some(ref overrides) = common_config.theme_overrides {
            $shared_config.theme_override(overrides)?;
        }
        if let Some(overrides) = common_config.icons_format {
            $shared_config.icons_format_override(overrides);
        }

        // Extract block-specific config
        let block_config = <$block_type as ConfigBlock>::Config::deserialize($block_config)
            .configuration_error("Failed to deserialize block config.")?;

        let mut block = $block_type::new($id, block_config, $shared_config, $update_request)?;
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

macro_rules! create_block_macro {
    (
        $id: expr, $name: expr, $block_config: expr, $shared_config: expr, $update_request: expr;
        $(
            $( #[cfg(feature = $feature: literal)] )?
            $mod: ident :: $block: ident;
        )*
    ) => {
        match $name {
            $(
                $( #[cfg(feature = $feature)] )?
                stringify!($mod) => {
                    pub use $mod::$block;
                    block!($block, $id, $block_config, $shared_config, $update_request)
                },
            )*
            other => Err(BlockError(other.to_string(), "Unknown block!".to_string())),
        }
    };
}

pub fn create_block(
    id: usize,
    name: &str,
    mut block_config: Value,
    mut shared_config: SharedConfig,
    update_request: Sender<Task>,
) -> Result<Box<dyn Block>> {
    create_block_macro! {
        id, name, block_config, shared_config, update_request;

        // Please keep these in alphabetical order.
        // apt::Apt;
        // backlight::Backlight;
        // battery::Battery;
        // bluetooth::Bluetooth;
        cpu::Cpu;
        custom::Custom;
        // custom_dbus::CustomDBus;
        // disk_space::DiskSpace;
        // docker::Docker;
        // focused_window::FocusedWindow;
        // github::Github;
        // hueshift::Hueshift;
        // ibus::IBus;
        // kdeconnect::KDEConnect;
        // keyboard_layout::KeyboardLayout;
        // load::Load;
        // #[cfg(feature="maildir")] maildir::Maildir;
        // memory::Memory;
        // music::Music;
        // net::Net;
        // networkmanager::NetworkManager;
        // notify::Notify;
        // #[cfg(feature="notmuch")] notmuch::Notmuch;
        // nvidia_gpu::NvidiaGpu;
        // pacman::Pacman;
        // pomodoro::Pomodoro;
        // sound::Sound;
        // speedtest::SpeedTest;
        // taskwarrior::Taskwarrior;
        // temperature::Temperature;
        // template::Template;
        // time::Time;
        // toggle::Toggle;
        // uptime::Uptime;
        // watson::Watson;
        // weather::Weather;
        // xrandr::Xrandr;
    }
}

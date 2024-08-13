#![warn(clippy::match_same_arms)]
#![warn(clippy::semicolon_if_nothing_returned)]
#![warn(clippy::unnecessary_wraps)]
#![allow(clippy::single_match)]
#![cfg_attr(docsrs, feature(doc_cfg))]

#[macro_use]
pub mod util;
pub mod blocks;
pub mod click;
pub mod config;
pub mod errors;
pub mod escape;
pub mod formatting;
pub mod icons;
mod netlink;
pub mod protocol;
mod signals;
mod subprocess;
pub mod themes;
pub mod widget;
mod wrappers;

pub use env_logger;
pub use serde_json;
pub use tokio;

use std::borrow::Cow;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use futures::stream::{FuturesUnordered, StreamExt};
use futures::Stream;
use tokio::process::Command;
use tokio::sync::{mpsc, Notify};

use crate::blocks::{BlockAction, BlockError, CommonApi};
use crate::click::{ClickHandler, MouseButton};
use crate::config::{BlockConfigEntry, Config, SharedConfig};
use crate::errors::*;
use crate::formatting::value::Value;
use crate::formatting::Format;
use crate::protocol::i3bar_block::I3BarBlock;
use crate::protocol::i3bar_event::{self, I3BarEvent};
use crate::signals::Signal;
use crate::widget::{State, Widget};

const APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);
const REQWEST_TIMEOUT: Duration = Duration::from_secs(10);

static REQWEST_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .user_agent(APP_USER_AGENT)
        .timeout(REQWEST_TIMEOUT)
        .build()
        .unwrap()
});

static REQWEST_CLIENT_IPV4: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .user_agent(APP_USER_AGENT)
        .local_address(Some(std::net::Ipv4Addr::UNSPECIFIED.into()))
        .timeout(REQWEST_TIMEOUT)
        .build()
        .unwrap()
});

type BoxedFuture<T> = Pin<Box<dyn Future<Output = T>>>;

type BoxedStream<T> = Pin<Box<dyn Stream<Item = T>>>;

type WidgetUpdatesSender = mpsc::UnboundedSender<(usize, Vec<u64>)>;

/// A feature-rich and resource-friendly replacement for i3status(1), written in Rust. The
/// i3status-rs program writes a stream of configurable "blocks" of system information (time,
/// battery status, volume, etc.) to standard output in the JSON format understood by i3bar(1) and
/// sway-bar(5).
#[derive(Debug, clap::Parser)]
#[clap(author, about, long_about, version = env!("VERSION"))]
pub struct CliArgs {
    /// Sets a TOML config file
    ///
    /// 1. If full absolute path given, then use it as is: `/home/foo/i3rs-config.toml`
    ///
    /// 2. If filename given, e.g. "custom_theme.toml", then first look in `$XDG_CONFIG_HOME/i3status-rust`
    ///
    /// 3. Then look for it in `$XDG_DATA_HOME/i3status-rust`
    ///
    /// 4. Otherwise look for it in `/usr/share/i3status-rust`
    #[clap(default_value = "config.toml")]
    pub config: String,
    /// Ignore any attempts by i3 to pause the bar when hidden/fullscreen
    #[clap(long = "never-pause")]
    pub never_pause: bool,
    /// Do not send the init sequence
    #[clap(hide = true, long = "no-init")]
    pub no_init: bool,
    /// The maximum number of blocking threads spawned by tokio
    #[clap(long = "threads", short = 'j', default_value = "2")]
    pub blocking_threads: usize,
}

pub struct BarState {
    config: Config,

    blocks: Vec<Block>,
    fullscreen_block: Option<usize>,
    running_blocks: FuturesUnordered<BoxedFuture<()>>,

    widget_updates_sender: WidgetUpdatesSender,
    blocks_render_cache: Vec<RenderedBlock>,

    request_sender: mpsc::UnboundedSender<Request>,
    request_receiver: mpsc::UnboundedReceiver<Request>,

    widget_updates_stream: BoxedStream<Vec<usize>>,
    signals_stream: BoxedStream<Signal>,
    events_stream: BoxedStream<I3BarEvent>,
}

#[derive(Debug)]
struct Request {
    block_id: usize,
    cmd: RequestCmd,
}

#[derive(Debug)]
enum RequestCmd {
    SetWidget(Widget),
    UnsetWidget,
    SetError(Error),
    SetDefaultActions(&'static [(MouseButton, Option<&'static str>, &'static str)]),
    SubscribeToActions(mpsc::UnboundedSender<BlockAction>),
}

#[derive(Debug, Clone)]
struct RenderedBlock {
    pub segments: Vec<I3BarBlock>,
    pub merge_with_next: bool,
}

#[derive(Debug)]
pub struct Block {
    id: usize,
    name: &'static str,

    update_request: Arc<Notify>,
    action_sender: Option<mpsc::UnboundedSender<BlockAction>>,

    click_handler: ClickHandler,
    default_actions: &'static [(MouseButton, Option<&'static str>, &'static str)],
    signal: Option<i32>,
    shared_config: SharedConfig,

    error_format: Format,
    error_fullscreen_format: Format,

    state: BlockState,
}

#[derive(Debug)]
enum BlockState {
    None,
    Normal { widget: Widget },
    Error { widget: Widget },
}

impl Block {
    fn notify_intervals(&self, tx: &WidgetUpdatesSender) {
        let intervals = match &self.state {
            BlockState::None => Vec::new(),
            BlockState::Normal { widget } | BlockState::Error { widget } => widget.intervals(),
        };
        let _ = tx.send((self.id, intervals));
    }

    fn send_action(&mut self, action: BlockAction) {
        if let Some(sender) = &self.action_sender {
            if sender.send(action).is_err() {
                self.action_sender = None;
            }
        }
    }

    fn set_error(&mut self, fullscreen: bool, error: Error) {
        let error = BlockError {
            block_id: self.id,
            block_name: self.name,
            error,
        };

        let mut widget = Widget::new()
            .with_state(State::Critical)
            .with_format(if fullscreen {
                self.error_fullscreen_format.clone()
            } else {
                self.error_format.clone()
            });
        widget.set_values(map! {
            "full_error_message" => Value::text(error.to_string()),
            [if let Some(v) = &error.error.message] "short_error_message" => Value::text(v.to_string()),
        });
        self.state = BlockState::Error { widget };
    }
}

impl BarState {
    pub fn new(config: Config) -> Self {
        let (request_sender, request_receiver) = mpsc::unbounded_channel();
        let (widget_updates_sender, widget_updates_stream) =
            formatting::scheduling::manage_widgets_updates();
        Self {
            blocks: Vec::new(),
            fullscreen_block: None,
            running_blocks: FuturesUnordered::new(),

            widget_updates_sender,
            blocks_render_cache: Vec::new(),

            request_sender,
            request_receiver,

            widget_updates_stream,
            signals_stream: signals::signals_stream(),
            events_stream: i3bar_event::events_stream(
                config.invert_scrolling,
                Duration::from_millis(config.double_click_delay),
            ),

            config,
        }
    }

    pub async fn spawn_block(&mut self, block_config: BlockConfigEntry) -> Result<()> {
        if let Some(cmd) = &block_config.common.if_command {
            // TODO: async
            if !Command::new("sh")
                .args(["-c", cmd])
                .output()
                .await
                .error("failed to run if_command")?
                .status
                .success()
            {
                return Ok(());
            }
        }

        let mut shared_config = self.config.shared.clone();

        // Overrides
        if let Some(icons_format) = block_config.common.icons_format {
            shared_config.icons_format = Arc::new(icons_format);
        }
        if let Some(theme_overrides) = block_config.common.theme_overrides {
            Arc::make_mut(&mut shared_config.theme).apply_overrides(theme_overrides)?;
        }
        if let Some(icons_overrides) = block_config.common.icons_overrides {
            Arc::make_mut(&mut shared_config.icons).apply_overrides(icons_overrides);
        }

        let update_request = Arc::new(Notify::new());

        let api = CommonApi {
            id: self.blocks.len(),
            update_request: update_request.clone(),
            request_sender: self.request_sender.clone(),
            error_interval: Duration::from_secs(block_config.common.error_interval),
        };

        let error_format = block_config
            .common
            .error_format
            .with_default_config(&self.config.error_format);
        let error_fullscreen_format = block_config
            .common
            .error_fullscreen_format
            .with_default_config(&self.config.error_fullscreen_format);

        let block = Block {
            id: self.blocks.len(),
            name: block_config.config.name(),

            update_request,
            action_sender: None,

            click_handler: block_config.common.click,
            default_actions: &[],
            signal: block_config.common.signal,
            shared_config,

            error_format,
            error_fullscreen_format,

            state: BlockState::None,
        };

        block_config.config.spawn(api, &mut self.running_blocks);

        self.blocks.push(block);
        self.blocks_render_cache.push(RenderedBlock {
            segments: Vec::new(),
            merge_with_next: block_config.common.merge_with_next,
        });

        Ok(())
    }

    fn process_request(&mut self, request: Request) {
        let block = &mut self.blocks[request.block_id];
        match request.cmd {
            RequestCmd::SetWidget(widget) => {
                block.state = BlockState::Normal { widget };
                if self.fullscreen_block == Some(request.block_id) {
                    self.fullscreen_block = None;
                }
            }
            RequestCmd::UnsetWidget => {
                block.state = BlockState::None;
                if self.fullscreen_block == Some(request.block_id) {
                    self.fullscreen_block = None;
                }
            }
            RequestCmd::SetError(error) => {
                block.set_error(self.fullscreen_block == Some(request.block_id), error);
            }
            RequestCmd::SetDefaultActions(actions) => {
                block.default_actions = actions;
            }
            RequestCmd::SubscribeToActions(action_sender) => {
                block.action_sender = Some(action_sender);
            }
        }
        block.notify_intervals(&self.widget_updates_sender);
    }

    fn render_block(&mut self, id: usize) -> Result<(), BlockError> {
        let block = &mut self.blocks[id];
        let data = &mut self.blocks_render_cache[id].segments;
        match &block.state {
            BlockState::None => {
                data.clear();
            }
            BlockState::Normal { widget } | BlockState::Error { widget, .. } => {
                *data = widget
                    .get_data(&block.shared_config, id)
                    .map_err(|error| BlockError {
                        block_id: id,
                        block_name: block.name,
                        error,
                    })?;
            }
        }
        Ok(())
    }

    fn render(&self) {
        if let Some(id) = self.fullscreen_block {
            protocol::print_blocks(&[&self.blocks_render_cache[id]], &self.config.shared);
        } else {
            protocol::print_blocks(&self.blocks_render_cache, &self.config.shared);
        }
    }

    async fn process_event(&mut self, restart: fn() -> !) -> Result<(), BlockError> {
        tokio::select! {
            // Poll blocks
            Some(()) = self.running_blocks.next() => (),
            // Receive messages from blocks
            Some(request) = self.request_receiver.recv() => {
                let id = request.block_id;
                self.process_request(request);
                self.render_block(id)?;
                self.render();
            }
            // Handle scheduled updates
            Some(ids) = self.widget_updates_stream.next() => {
                for id in ids {
                    self.render_block(id)?;
                }
                self.render();
            }
            // Handle clicks
            Some(event) = self.events_stream.next() => {
                let block = self.blocks.get_mut(event.id).expect("Events receiver: ID out of bounds");
                match &mut block.state {
                    BlockState::None => (),
                    BlockState::Normal { .. } => {
                        let result = block.click_handler.handle(&event).await.map_err(|error| BlockError {
                            block_id: event.id,
                            block_name: block.name,
                            error,
                        })?;
                        match result {
                            Some(post_actions) => {
                                if let Some(action) = post_actions.action {
                                    block.send_action(Cow::Owned(action));
                                }
                                if post_actions.update {
                                    block.update_request.notify_one();
                                }
                            }
                            None => {
                                if let Some((_, _, action)) = block.default_actions
                                    .iter()
                                    .find(|(btn, widget, _)| *btn == event.button && *widget == event.instance.as_deref()) {
                                    block.send_action(Cow::Borrowed(action));
                                }
                            }
                        }
                    }
                    BlockState::Error { widget } => {
                        if self.fullscreen_block == Some(event.id) {
                            self.fullscreen_block = None;
                            widget.set_format(block.error_format.clone());
                        } else {
                            self.fullscreen_block = Some(event.id);
                            widget.set_format(block.error_fullscreen_format.clone());
                        }
                        block.notify_intervals(&self.widget_updates_sender);
                        self.render_block(event.id)?;
                        self.render();
                    }
                }
            }
            // Handle signals
            Some(signal) = self.signals_stream.next() => match signal {
                Signal::Usr1 => {
                    for block in &self.blocks {
                        block.update_request.notify_one();
                    }
                }
                Signal::Usr2 => restart(),
                Signal::Custom(signal) => {
                    for block in &self.blocks {
                        if block.signal == Some(signal) {
                            block.update_request.notify_one();
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub async fn run_event_loop(mut self, restart: fn() -> !) -> Result<(), BlockError> {
        loop {
            if let Err(error) = self.process_event(restart).await {
                let block = &mut self.blocks[error.block_id];

                if matches!(block.state, BlockState::Error { .. }) {
                    // This should never happen. If this code runs, it could mean that we
                    // got an error while trying to display and error. We better stop here.
                    return Err(error);
                }

                block.set_error(self.fullscreen_block == Some(block.id), error.error);
                block.notify_intervals(&self.widget_updates_sender);

                self.render_block(error.block_id)?;
                self.render();
            }
        }
    }
}

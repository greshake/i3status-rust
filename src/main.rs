#![warn(clippy::match_same_arms)]
#![warn(clippy::semicolon_if_nothing_returned)]
#![warn(clippy::unnecessary_wraps)]

#[macro_use]
mod util;
mod blocks;
mod click;
mod config;
mod errors;
mod escape;
mod formatting;
mod icons;
mod netlink;
mod protocol;
mod signals;
mod subprocess;
mod themes;
mod widget;
mod wrappers;

use clap::Parser;
use formatting::value::Value;
use futures::future::{abortable, FutureExt};
use futures::stream::futures_unordered::FuturesUnordered;
use futures::stream::{AbortHandle, Stream, StreamExt};
use once_cell::sync::Lazy;
use protocol::i3bar_block::I3BarBlock;
use protocol::i3bar_event::I3BarEvent;
use std::borrow::Cow;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::mpsc;

use blocks::{BlockEvent, BlockFuture, CommonApi};
use click::{ClickHandler, MouseButton};
use config::SharedConfig;
use config::{BlockConfigEntry, Config};
use errors::*;
use escape::CollectEscaped;
use formatting::{scheduling, Format};
use protocol::i3bar_event::events_stream;
use signals::{signals_stream, Signal};
use widget::{State, Widget};

pub type BoxedFuture<T> = Pin<Box<dyn Future<Output = T>>>;
pub type BoxedStream<T> = Pin<Box<dyn Stream<Item = T>>>;

pub static REQWEST_CLIENT: Lazy<reqwest::Client> = Lazy::new(|| {
    const APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);
    const REQWEST_TIMEOUT: Duration = Duration::from_secs(10);
    reqwest::Client::builder()
        .user_agent(APP_USER_AGENT)
        .timeout(REQWEST_TIMEOUT)
        .build()
        .unwrap()
});

#[derive(Debug, Parser)]
#[clap(author, about, version = env!("VERSION"))]
struct CliArgs {
    /// Sets a TOML config file
    #[clap(default_value = "config.toml")]
    config: String,
    /// Ignore any attempts by i3 to pause the bar when hidden/fullscreen
    #[clap(long = "never-pause")]
    never_pause: bool,
    /// Do not send the init sequence
    #[clap(long = "no-init")]
    no_init: bool,
    /// The maximum number of blocking threads spawned by tokio
    #[clap(long = "threads", short = 'j', default_value = "2")]
    blocking_threads: usize,
}

fn main() {
    env_logger::init();
    let args = CliArgs::parse();
    let blocking_threads = args.blocking_threads;

    if !args.no_init {
        protocol::init(args.never_pause);
    }

    let result = tokio::runtime::Builder::new_current_thread()
        .max_blocking_threads(blocking_threads)
        .enable_all()
        .build()
        .unwrap()
        .block_on(async move {
            let config_path = util::find_file(&args.config, None, Some("toml"))
                .or_error(|| format!("Configuration file '{}' not found", args.config))?;
            let mut config: Config = util::deserialize_toml_file(&config_path)?;
            let blocks = std::mem::take(&mut config.blocks);
            let mut bar = BarState::new(config);
            for block_config in blocks {
                bar.spawn_block(block_config).await?;
            }
            bar.run_event_loop().await
        });
    if let Err(error) = result {
        let error_widget = Widget::new()
            .with_text(error.to_string().chars().collect_pango_escaped())
            .with_state(State::Critical);

        println!(
            "{},",
            serde_json::to_string(&error_widget.get_data(&Default::default(), 0).unwrap()).unwrap()
        );
        eprintln!("\n\n{error}\n\n");
        dbg!(error);

        // Wait for USR2 signal to restart
        signal_hook::iterator::Signals::new([signal_hook::consts::SIGUSR2])
            .unwrap()
            .forever()
            .next()
            .unwrap();
        restart();
    }
}

#[derive(Debug)]
pub struct Block {
    id: usize,

    event_sender: Option<mpsc::Sender<BlockEvent>>,
    widget_updates_sender: mpsc::UnboundedSender<(usize, Vec<u64>)>,
    abort_handle: AbortHandle,

    click_handler: ClickHandler,
    default_actions: &'static [(MouseButton, Option<&'static str>, &'static str)],
    signal: Option<i32>,
    shared_config: SharedConfig,

    error_format: Format,
    error_fullscreen_format: Format,

    state: BlockState,
}

impl Block {
    fn abort(&mut self) {
        self.abort_handle.abort();
        self.event_sender = None;
        self.state = BlockState::None;
    }

    fn notify_intervals(&self) {
        let widget = match &self.state {
            BlockState::None => return,
            BlockState::Normal { widget } | BlockState::Error { widget } => widget,
        };
        let _ = self
            .widget_updates_sender
            .send((self.id, widget.intervals()));
    }

    fn set_error(&mut self, fullscreen: bool, error: Error) {
        let mut widget = Widget::new()
            .with_state(State::Critical)
            .with_format(if fullscreen {
                self.error_fullscreen_format.clone()
            } else {
                self.error_format.clone()
            });
        widget.set_values(map! {
            "full_error_message" => Value::text(error.to_string()),
            [if let Some(v) = &error.message] "short_error_message" => Value::text(v.to_string()),
        });
        self.state = BlockState::Error { widget };
    }
}

#[derive(Debug)]
pub enum BlockState {
    None,
    Normal { widget: Widget },
    Error { widget: Widget },
}

#[derive(Debug)]
pub struct Request {
    pub block_id: usize,
    pub cmd: RequestCmd,
}

#[derive(Debug)]
pub enum RequestCmd {
    SetWidget(Widget),
    UnsetWidget,
    SetError(Error),
    SetDefaultActions(&'static [(MouseButton, Option<&'static str>, &'static str)]),
}

struct BarState {
    config: Config,

    blocks: Vec<(Block, &'static str)>,
    fullscreen_block: Option<usize>,
    running_blocks: FuturesUnordered<BlockFuture>,

    widget_updates_stream: BoxedStream<Vec<usize>>,
    widget_updates_sender: mpsc::UnboundedSender<(usize, Vec<u64>)>,
    blocks_render_cache: Vec<Vec<I3BarBlock>>,

    request_sender: mpsc::Sender<Request>,
    request_receiver: mpsc::Receiver<Request>,

    signals_stream: BoxedStream<Signal>,
    events_stream: BoxedStream<I3BarEvent>,
}

impl BarState {
    fn new(config: Config) -> Self {
        let (request_sender, request_receiver) = mpsc::channel(64);
        let (widget_updates_sender, widget_updates_stream) = scheduling::manage_widgets_updates();
        Self {
            blocks: Vec::new(),
            fullscreen_block: None,
            running_blocks: FuturesUnordered::new(),

            widget_updates_stream,
            widget_updates_sender,
            blocks_render_cache: Vec::new(),

            request_sender,
            request_receiver,

            signals_stream: signals_stream(),
            events_stream: events_stream(
                config.invert_scrolling,
                Duration::from_millis(config.double_click_delay),
            ),

            config,
        }
    }

    async fn spawn_block(&mut self, block_config: BlockConfigEntry) -> Result<()> {
        if let Some(cmd) = &block_config.common.if_command {
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

        let (event_sender, event_receiver) = mpsc::channel(64);

        let api = CommonApi {
            id: self.blocks.len(),
            shared_config: shared_config.clone(),
            event_receiver,

            request_sender: self.request_sender.clone(),

            error_interval: Duration::from_secs(block_config.common.error_interval),
        };

        let error_format = block_config
            .common
            .error_format
            .with_default(&self.config.error_format)?;
        let error_fullscreen_format = block_config
            .common
            .error_fullscreen_format
            .with_default(&self.config.error_fullscreen_format)?;

        let block_name = block_config.config.name();
        let (block_fut, abort_handle) = abortable(block_config.config.run(api));

        let block = Block {
            id: self.blocks.len(),

            event_sender: Some(event_sender),
            widget_updates_sender: self.widget_updates_sender.clone(),
            abort_handle,

            click_handler: block_config.common.click,
            default_actions: &[],
            signal: block_config.common.signal,
            shared_config,

            error_format,
            error_fullscreen_format,

            state: BlockState::None,
        };

        self.running_blocks
            .push(Box::pin(block_fut.map(|res| match res {
                Ok(res) => res,
                Err(_aborted) => Ok(()),
            })));
        self.blocks.push((block, block_name));
        self.blocks_render_cache.push(Vec::new());
        Ok(())
    }

    fn process_request(&mut self, request: Request) {
        let block = &mut self.blocks[request.block_id].0;
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
        }
        block.notify_intervals();
    }

    fn render_block(&mut self, id: usize) -> Result<()> {
        let (block, block_type) = &mut self.blocks[id];
        let data = &mut self.blocks_render_cache[id];
        match &block.state {
            BlockState::None => {
                data.clear();
            }
            BlockState::Normal { widget } | BlockState::Error { widget, .. } => {
                *data = widget
                    .get_data(&block.shared_config, id)
                    .in_block(block_type, id)?;
            }
        }
        Ok(())
    }

    fn render(&self) {
        if let Some(id) = self.fullscreen_block {
            protocol::print_blocks(&[self.blocks_render_cache[id].clone()], &self.config.shared);
        } else {
            protocol::print_blocks(&self.blocks_render_cache, &self.config.shared);
        }
    }

    async fn process_event(&mut self) -> Result<()> {
        tokio::select! {
            // Handle blocks' errors
            Some(block_result) = self.running_blocks.next() => {
                block_result
            }
            // Receive messages from blocks
            Some(request) = self.request_receiver.recv() => {
                let id = request.block_id;
                self.process_request(request);
                self.render_block(id)?;
                self.render();
                Ok(())
            }
            // Handle scheduled updates
            Some(ids) = self.widget_updates_stream.next() => {
                for id in ids {
                    self.render_block(id)?;
                }
                self.render();
                Ok(())
            }
            // Handle clicks
            Some(event) = self.events_stream.next() => {
                let (block, block_type) = self.blocks.get_mut(event.id).error("Events receiver: ID out of bounds")?;
                match &mut block.state {
                    BlockState::None => (),
                    BlockState::Normal { .. } => {
                        let post_actions = block.click_handler.handle(&event).await.in_block(block_type, event.id)?;
                        if let Some(sender) = &block.event_sender {
                            if let Some(action) = post_actions.action {
                                let _ = sender.send(BlockEvent::Action(Cow::Owned(action))).await;
                            } else if let Some((_, _, action)) = block.default_actions
                                .iter()
                                .find(|(btn, widget, _)| *btn == event.button && *widget == event.instance.as_deref()) {
                                let _ = sender.send(BlockEvent::Action(Cow::Borrowed(action))).await;
                            }
                            if post_actions.update {
                                let _ = sender.send(BlockEvent::UpdateRequest).await;
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
                        block.notify_intervals();
                        self.render_block(event.id)?;
                        self.render();
                    }
                }
                Ok(())
            }
            // Handle signals
            Some(signal) = self.signals_stream.next() => match signal {
                Signal::Usr1 => {
                    for (block, _) in &self.blocks {
                        if let Some(sender) = &block.event_sender {
                            let _ = sender.send(BlockEvent::UpdateRequest).await;
                        }
                    }
                    Ok(())
                }
                Signal::Usr2 => restart(),
                Signal::Custom(signal) => {
                    for (block, _) in &self.blocks {
                        if let Some(sender) = &block.event_sender {
                            if block.signal == Some(signal) {
                                let _ = sender.send(BlockEvent::UpdateRequest).await;
                            }
                        }
                    }
                    Ok(())
                }
            }
        }
    }

    async fn run_event_loop(mut self) -> Result<()> {
        loop {
            if let Err(error) = self.process_event().await {
                match error.block {
                    Some((_, id)) => {
                        let block = &mut self.blocks[id].0;

                        if matches!(block.state, BlockState::Error { .. }) {
                            // This should never happen. If this code runs, it cound mean that we
                            // got an error while trying to display and error. We better stop here.
                            return Err(error);
                        }

                        block.abort();
                        block.set_error(self.fullscreen_block == Some(id), error);
                        block.notify_intervals();

                        self.render_block(id)?;
                        self.render();
                    }
                    None => return Err(error),
                }
            }
        }
    }
}

/// Restart in-place
fn restart() -> ! {
    use std::env;
    use std::ffi::CString;
    use std::os::unix::ffi::OsStringExt;

    // On linux this line should be OK
    let exe = CString::new(env::current_exe().unwrap().into_os_string().into_vec()).unwrap();

    // Get current arguments
    let mut arg: Vec<CString> = env::args_os()
        .map(|a| CString::new(a.into_vec()).unwrap())
        .collect();

    // Add "--no-init" argument if not already added
    let no_init_arg = CString::new("--no-init").unwrap();
    if !arg.iter().any(|a| *a == no_init_arg) {
        arg.push(no_init_arg);
    }

    // Restart
    nix::unistd::execvp(&exe, &arg).unwrap();
    unreachable!();
}

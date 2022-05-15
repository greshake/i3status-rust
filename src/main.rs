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
use futures::future::{abortable, FutureExt};
use futures::stream::futures_unordered::FuturesUnordered;
use futures::stream::{AbortHandle, Stream, StreamExt};
use once_cell::sync::Lazy;
use protocol::i3bar_block::I3BarBlock;
use protocol::i3bar_event::I3BarEvent;
use smallvec::SmallVec;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::sync::oneshot::Sender as OneshotSender;

use blocks::{BlockEvent, BlockFuture, BlockType, CommonApi, CommonConfig};
use click::ClickHandler;
use config::Config;
use config::SharedConfig;
use errors::*;
use formatting::scheduling;
use formatting::{Format, Values};
use protocol::i3bar_event::events_stream;
use signals::{signals_stream, Signal};
use widget::{State, Widget};

pub type BoxedFuture<T> = Pin<Box<dyn Future<Output = T>>>;
pub type BoxedStream<T> = Pin<Box<dyn Stream<Item = T>>>;

pub static REQWEST_CLIENT: Lazy<reqwest::Client> = Lazy::new(|| {
    const APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);
    reqwest::Client::builder()
        .user_agent(APP_USER_AGENT)
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
    /// The DBUS name
    #[clap(long = "dbus-name", default_value = "rs.i3status")]
    dbus_name: String,
}

fn main() {
    // #[cfg(feature = "console")]
    // {
    // console_subscriber::init();
    // }

    let args = CliArgs::parse();
    let blocking_threads = args.blocking_threads;

    if !args.no_init {
        protocol::init(args.never_pause);
    }

    let result = (|| {
        // Read & parse the config file
        let config_path = util::find_file(&args.config, None, Some("toml"))
            .or_error(|| format!("Configuration file '{}' not found", args.config))?;
        let config: Config = util::deserialize_toml_file(&config_path).config_error()?;

        // Run main loop
        tokio::runtime::Builder::new_current_thread()
            .max_blocking_threads(blocking_threads)
            .enable_all()
            .build()
            .unwrap()
            .block_on(async move {
                let mut bar = BarState::new(&config, args);
                for block_config in config.blocks {
                    bar.spawn_block(block_config)?;
                }
                bar.run_event_loop().await
            })
    })();

    if let Err(error) = result {
        let error_widget = Widget::new(0, Default::default(), None).with_text(error.to_string());
        println!(
            "{},",
            serde_json::to_string(&error_widget.get_data().unwrap()).unwrap()
        );
        eprintln!("\n\n{}\n\n", error);
        dbg!(error);

        // Wait for USR2 signal to restart
        signal_hook::iterator::Signals::new(&[signal_hook::consts::SIGUSR2])
            .unwrap()
            .forever()
            .next()
            .unwrap();
        restart();
    }
}

pub struct RunningBlock {
    id: usize,

    event_sender: mpsc::Sender<BlockEvent>,
    abort_handle: AbortHandle,
    click_handler: ClickHandler,
    signal: Option<i32>,

    hidden: bool,
    widget: Widget,
}

pub struct FailedBlock {
    id: usize,
    error_widget: Widget,
    error: Error,
}

pub enum Block {
    Running(RunningBlock),
    Failed(FailedBlock),
}

#[derive(Debug)]
pub struct Request {
    pub block_id: usize,
    pub cmds: SmallVec<[RequestCmd; 4]>,
}

#[derive(Debug)]
pub enum RequestCmd {
    Hide,
    Show,

    SetIcon(String),
    SetState(State),
    SetText(String),
    SetTexts(String, String),

    SetFormat(Format),
    SetValues(Values),

    SetFullScreen(bool),

    Preserve,
    Restore,

    GetDbusConnection(OneshotSender<Result<zbus::Connection>>),
    GetSystemDbusConnection(OneshotSender<Result<zbus::Connection>>),

    Noop,
}

struct BarState {
    shared_config: SharedConfig,
    cli_args: CliArgs,

    blocks: Vec<(Block, BlockType)>,
    fullscreen_block: Option<usize>,
    running_blocks: FuturesUnordered<BlockFuture>,

    widget_updates_stream: BoxedStream<Vec<usize>>,
    widget_updates_sender: mpsc::UnboundedSender<(usize, Vec<u64>)>,
    blocks_render_cache: Vec<Vec<I3BarBlock>>,

    request_sender: mpsc::Sender<Request>,
    request_receiver: mpsc::Receiver<Request>,

    signals_stream: BoxedStream<Signal>,
    events_stream: BoxedStream<I3BarEvent>,

    dbus_connection: Option<zbus::Connection>,
    system_dbus_connection: Option<zbus::Connection>,
}

impl BarState {
    fn new(config: &Config, cli: CliArgs) -> Self {
        let (request_sender, request_receiver) = mpsc::channel(64);
        let (widget_updates_sender, widget_updates_stream) = scheduling::manage_widgets_updates();
        Self {
            shared_config: config.shared.clone(),
            cli_args: cli,

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

            dbus_connection: None,
            system_dbus_connection: None,
        }
    }

    fn spawn_block(&mut self, mut block_config: toml::Value) -> Result<()> {
        let common_config = CommonConfig::new(&mut block_config)?;
        let block_type = common_config.block;
        let mut shared_config = self.shared_config.clone();

        // Overrides
        if let Some(icons_format) = common_config.icons_format {
            shared_config.icons_format = Arc::new(icons_format);
        }
        if let Some(theme_overrides) = common_config.theme_overrides {
            Arc::make_mut(&mut shared_config.theme).apply_overrides(&theme_overrides)?;
        }

        let (event_sender, event_receiver) = mpsc::channel(64);

        let id = self.blocks.len();
        let api = CommonApi {
            id,
            shared_config: shared_config.clone(),
            event_receiver,

            request_sender: self.request_sender.clone(),
            cmd_buf: SmallVec::new(),

            error_interval: Duration::from_secs(common_config.error_interval),
            error_format: common_config.error_format,
        };

        let (block_fut, abort_handle) = abortable(block_type.run(block_config, api));

        let block = Block::Running(RunningBlock {
            id,

            event_sender,
            abort_handle,
            click_handler: common_config.click,
            signal: common_config.signal,

            hidden: false,
            widget: Widget::new(id, shared_config, Some(self.widget_updates_sender.clone())),
        });

        self.running_blocks
            .push(Box::pin(block_fut.map(|res| match res {
                Ok(res) => res,
                Err(_aborted) => Ok(()),
            })));
        self.blocks.push((block, block_type));
        self.blocks_render_cache.push(Vec::new());
        Ok(())
    }

    async fn process_request(&mut self, request: Request) -> Result<()> {
        let (block, block_type) = self
            .blocks
            .get_mut(request.block_id)
            .error("Message receiver: ID out of bounds")?;
        let block = match block {
            Block::Running(block) => block,
            Block::Failed(_) => {
                // Ignore requests from failed blocks
                return Ok(());
            }
        };
        for cmd in request.cmds {
            match cmd {
                RequestCmd::Hide => block.hidden = true,
                RequestCmd::Show => block.hidden = false,
                RequestCmd::SetIcon(icon) => block.widget.icon = icon,
                RequestCmd::SetText(text) => block.widget.set_text(text),
                RequestCmd::SetTexts(full, short) => block.widget.set_texts(full, short),
                RequestCmd::SetState(state) => block.widget.state = state,
                RequestCmd::SetFormat(format) => block.widget.set_format(format),
                RequestCmd::SetValues(values) => block.widget.set_values(values),
                RequestCmd::SetFullScreen(value) => {
                    if self.fullscreen_block.is_none() && value {
                        self.fullscreen_block = Some(block.id);
                    } else if self.fullscreen_block == Some(block.id) && !value {
                        self.fullscreen_block = None;
                    }
                }
                RequestCmd::Preserve => block.widget.preserve(),
                RequestCmd::Restore => block.widget.restore(),
                RequestCmd::GetDbusConnection(tx) => match &self.dbus_connection {
                    Some(conn) => {
                        let _ = tx.send(Ok(conn.clone()));
                    }
                    None => {
                        let conn = util::new_dbus_connection().await?;
                        conn.request_name(self.cli_args.dbus_name.as_str())
                            .await
                            .error("Failed to reuqest DBus name")?;
                        self.dbus_connection = Some(conn.clone());
                        let _ = tx.send(Ok(conn));
                    }
                },
                RequestCmd::GetSystemDbusConnection(tx) => match &self.system_dbus_connection {
                    Some(conn) => {
                        let _ = tx.send(Ok(conn.clone()));
                    }
                    None => {
                        let conn = util::new_system_dbus_connection().await?;
                        self.system_dbus_connection = Some(conn.clone());
                        let _ = tx.send(Ok(conn));
                    }
                },
                RequestCmd::Noop => (),
            }
        }

        let data = &mut self.blocks_render_cache[block.id];
        if !block.hidden {
            *data = block.widget.get_data().in_block(*block_type, block.id)?;
        } else {
            data.clear();
        }

        Ok(())
    }

    fn render(&self) {
        if let Some(id) = self.fullscreen_block {
            protocol::print_blocks(&[self.blocks_render_cache[id].clone()], &self.shared_config);
        } else {
            protocol::print_blocks(&self.blocks_render_cache, &self.shared_config);
        }
    }

    async fn process_event(&mut self) -> Result<()> {
        tokio::select! {
            // Handle blocks' errors
            Some(block_result) = self.running_blocks.next() => {
                block_result
            }
            // Recieve messages from blocks
            Some(request) = self.request_receiver.recv() => {
                self.process_request(request).await?;
                self.render();
                Ok(())
            }
            // Handle scheduled updates
            Some(ids) = self.widget_updates_stream.next() => {
                for id in ids {
                    let data = &mut self.blocks_render_cache[id];
                    let (block, block_type) = &self.blocks[id];
                    if let Block::Running(block) = block {
                        if !block.hidden {
                            *data = block.widget.get_data().in_block(*block_type, id)?;
                        } else {
                            data.clear();
                        }
                    }
                }
                self.render();
                Ok(())
            }
            // Handle clicks
            Some(event) = self.events_stream.next() => {
                let (block, block_type) = self.blocks.get_mut(event.id).error("Events receiver: ID out of bounds")?;
                match block {
                    Block::Running(block) => {
                        if block.click_handler.handle(event.button).await.in_block(*block_type, event.id)? {
                                let _ = block.event_sender.send(BlockEvent::Click(event)).await;
                        }
                    }
                    Block::Failed(block) => {
                        let text = if self.fullscreen_block == Some(block.id) {
                            self.fullscreen_block = None;
                            block.error.message.as_deref().unwrap_or("Error").into()
                        } else {
                            self.fullscreen_block = Some(block.id);
                            block.error.to_string()
                        };
                        block.error_widget.set_text(text);
                        self.blocks_render_cache[block.id] = block.error_widget.get_data()?;
                        self.render();
                    }
                }
                Ok(())
            }
            // Handle signals
            Some(signal) = self.signals_stream.next() => match signal {
                Signal::Usr1 => {
                    for (block, _) in &self.blocks {
                        if let Block::Running(block) = block {
                            let _ = block.event_sender.send(BlockEvent::UpdateRequest).await;
                        }
                    }
                    Ok(())
                }
                Signal::Usr2 => restart(),
                Signal::Custom(signal) => {
                    for (block, _) in &self.blocks {
                        if let Block::Running(block) = block {
                            if block.signal == Some(signal) {
                                let _ = block.event_sender.send(BlockEvent::UpdateRequest).await;
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
                        if let Block::Running(block) = &self.blocks[id].0 {
                            block.abort_handle.abort();
                        }
                        let block = FailedBlock {
                            id,
                            error_widget: Widget::new(id, self.shared_config.clone(), None)
                                .with_state(State::Critical)
                                .with_text(error.message.as_deref().unwrap_or("Error").into()),
                            error,
                        };

                        self.blocks_render_cache[block.id] = block.error_widget.get_data()?;

                        self.render();

                        self.blocks[id].0 = Block::Failed(block);
                        self.fullscreen_block = None;
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

use std::borrow::Cow;
use std::sync::Arc;
use std::time::Duration;

use futures::future::abortable;
use futures::stream::futures_unordered::FuturesUnordered;
use futures::stream::{AbortHandle, Abortable, StreamExt};

use tokio::process::Command;
use tokio::sync::mpsc;

use clap::Parser;

use i3status_rs::blocks::{BlockEvent, BlockFuture, CommonApi};
use i3status_rs::click::{ClickHandler, MouseButton};
use i3status_rs::config::SharedConfig;
use i3status_rs::config::{BlockConfigEntry, Config};
use i3status_rs::errors::*;
use i3status_rs::escape::CollectEscaped;
use i3status_rs::formatting::value::Value;
use i3status_rs::formatting::{scheduling, Format};
use i3status_rs::protocol::i3bar_event::{events_stream, I3BarEvent};
use i3status_rs::signals::{signals_stream, Signal};
use i3status_rs::util::map;
use i3status_rs::widget::{State, Widget};
use i3status_rs::*;

type WidgetUpdatesSender = mpsc::UnboundedSender<(usize, Vec<u64>)>;

fn main() {
    env_logger::init();

    let args = i3status_rs::CliArgs::parse();
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

    fn notify_intervals(&self, tx: &WidgetUpdatesSender) {
        let intervals = match &self.state {
            BlockState::None => return,
            BlockState::Normal { widget } | BlockState::Error { widget } => widget.intervals(),
        };
        let _ = tx.send((self.id, intervals));
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

struct BarState {
    config: Config,

    blocks: Vec<(Block, &'static str)>,
    fullscreen_block: Option<usize>,
    running_blocks: FuturesUnordered<Abortable<BlockFuture>>,

    widget_updates_stream: BoxedStream<Vec<usize>>,
    widget_updates_sender: WidgetUpdatesSender,
    blocks_render_cache: Vec<RenderedBlock>,

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
            .with_default_config(&self.config.error_format);
        let error_fullscreen_format = block_config
            .common
            .error_fullscreen_format
            .with_default_config(&self.config.error_fullscreen_format);

        let block_name = block_config.config.name();
        let (block_fut, abort_handle) = abortable(block_config.config.run(api));

        let block = Block {
            id: self.blocks.len(),

            event_sender: Some(event_sender),
            abort_handle,

            click_handler: block_config.common.click,
            default_actions: &[],
            signal: block_config.common.signal,
            shared_config,

            error_format,
            error_fullscreen_format,

            state: BlockState::None,
        };

        self.running_blocks.push(block_fut);
        self.blocks.push((block, block_name));
        self.blocks_render_cache.push(RenderedBlock {
            segments: Vec::new(),
            merge_with_next: block_config.common.merge_with_next,
        });

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
        block.notify_intervals(&self.widget_updates_sender);
    }

    fn render_block(&mut self, id: usize) -> Result<()> {
        let (block, block_type) = &mut self.blocks[id];
        let data = &mut self.blocks_render_cache[id].segments;
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
            protocol::print_blocks(&[&self.blocks_render_cache[id]], &self.config.shared);
        } else {
            protocol::print_blocks(&self.blocks_render_cache, &self.config.shared);
        }
    }

    async fn process_event(&mut self) -> Result<()> {
        tokio::select! {
            // Handle blocks' errors
            Some(block_result) = self.running_blocks.next() => {
                match block_result {
                    Ok(res) => res,
                    Err(_aborted) => Ok(()),
                }
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
                            match post_actions {
                                Some(post_actions) => {
                                    if let Some(action) = post_actions.action {
                                        let _ = sender.send(BlockEvent::Action(Cow::Owned(action))).await;
                                    }
                                    if post_actions.update {
                                        let _ = sender.send(BlockEvent::UpdateRequest).await;
                                    }
                                }
                                None => {
                                    if let Some((_, _, action)) = block.default_actions
                                        .iter()
                                        .find(|(btn, widget, _)| *btn == event.button && *widget == event.instance.as_deref()) {
                                        let _ = sender.send(BlockEvent::Action(Cow::Borrowed(action))).await;
                                    }
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
                            // This should never happen. If this code runs, it could mean that we
                            // got an error while trying to display and error. We better stop here.
                            return Err(error);
                        }

                        block.abort();
                        block.set_error(self.fullscreen_block == Some(id), error);
                        block.notify_intervals(&self.widget_updates_sender);

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

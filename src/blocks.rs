//! The collection of blocks

pub mod prelude;

use crate::BoxedFuture;
use futures::future::FutureExt;
use serde::Deserialize;
use tokio::sync::mpsc;

use std::future::Future;
use std::time::Duration;

use crate::config::SharedConfig;
use crate::errors::*;
use crate::protocol::i3bar_event::I3BarEvent;
use crate::widget::Widget;
use crate::{Request, RequestCmd};

macro_rules! define_blocks {
    {
        $( $(#[cfg($attr: meta)])? $block: ident $(,)? )*
    } => {
        $(
            $(#[cfg($attr)])?
            pub mod $block;
        )*

        #[derive(Debug, Deserialize)]
        #[serde(tag = "block")]
        #[serde(deny_unknown_fields)]
        pub enum BlockConfig {
            $(
                $(#[cfg($attr)])?
                #[allow(non_camel_case_types)]
                $block {
                    #[serde(flatten)]
                    config: $block::Config,
                },
            )*
        }

        impl BlockConfig {
            pub fn name(&self) -> &'static str {
                match self {
                    $(
                        $(#[cfg($attr)])?
                        Self::$block { .. } => stringify!($block),
                    )*
                }
            }

            pub fn run(self, api: CommonApi) -> BlockFuture {
                let id = api.id;
                match self {
                    $(
                        $(#[cfg($attr)])?
                        Self::$block { config } => {
                            $block::run(config, api).map(move |e| e.in_block(stringify!($block), id)).boxed_local()
                        }
                    )*
                }
            }
        }
    };
}

define_blocks!(
    apt,
    backlight,
    battery,
    bluetooth,
    cpu,
    custom,
    custom_dbus,
    disk_space,
    dnf,
    docker,
    external_ip,
    focused_window,
    github,
    hueshift,
    kdeconnect,
    load,
    #[cfg(feature = "maildir")]
    maildir,
    menu,
    memory,
    music,
    net,
    notify,
    #[cfg(feature = "notmuch")]
    notmuch,
    nvidia_gpu,
    pacman,
    pomodoro,
    rofication,
    sound,
    speedtest,
    keyboard_layout,
    taskwarrior,
    temperature,
    time,
    toggle,
    uptime,
    watson,
    weather,
    xrandr,
);

pub type BlockFuture = BoxedFuture<Result<()>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockEvent {
    Click(I3BarEvent),
    UpdateRequest,
}

pub struct CommonApi {
    pub id: usize,
    pub shared_config: SharedConfig,
    pub event_receiver: mpsc::Receiver<BlockEvent>,

    pub request_sender: mpsc::Sender<Request>,

    pub error_interval: Duration,
}

impl CommonApi {
    /// Sends the widget to be displayed.
    pub async fn set_widget(&self, widget: &Widget) -> Result<()> {
        self.request_sender
            .send(Request {
                block_id: self.id,
                cmd: RequestCmd::SetWidget(widget.clone()),
            })
            .await
            .error("Failed to send Request")
    }

    /// Hides the block. Send new widget to make it visible again.
    pub async fn hide(&self) -> Result<()> {
        self.request_sender
            .send(Request {
                block_id: self.id,
                cmd: RequestCmd::UnsetWidget,
            })
            .await
            .error("Failed to send Request")
    }

    /// Sends the error to be displayed.
    pub async fn set_error(&self, error: Error) -> Result<()> {
        self.request_sender
            .send(Request {
                block_id: self.id,
                cmd: RequestCmd::SetError(error),
            })
            .await
            .error("Failed to send Request")
    }

    /// Receive the next event, such as click notification or update request.
    ///
    /// This method should be called regularly to avoid sender blocking. Currently, the runtime is
    /// single threaded, so full channel buffer will cause a deadlock. If receiving events is
    /// impossible / meaningless, call `event_receiver.close()`.
    ///
    /// # Cancel safety
    ///
    /// This method is cancel safe.
    ///
    /// # Panics
    ///
    /// Panics if event sender is closed
    ///
    /// # Examples
    ///
    /// ```
    /// tokio::select! {
    ///     _ = timer.tick() => (),
    ///     event = api.event() => match event {
    ///         // ...
    ///         _ => (),
    ///     }
    /// }
    /// ```
    pub async fn event(&mut self) -> BlockEvent {
        match self.event_receiver.recv().await {
            Some(event) => event,
            None => panic!("events stream ended"),
        }
    }

    /// Wait for the next update request.
    ///
    /// The update request can be send by clicking on the block (with `update=true`) or sending a
    /// signal.
    ///
    /// # Cancel safety
    ///
    /// This method is cancel safe.
    ///
    /// # Panics
    ///
    /// Panics if event sender is closed
    ///
    /// # Examples
    ///
    /// ```
    /// tokio::select! {
    ///     _ = timer.tick() => (),
    ///     _ = api.wait_for_update_request() => (),
    /// }
    /// ```
    pub async fn wait_for_update_request(&mut self) {
        while self.event().await != BlockEvent::UpdateRequest {}
    }

    pub fn get_icon(&self, icon: &str) -> Result<String> {
        self.shared_config
            .get_icon(icon)
            .or_error(|| format!("Icon '{}' not found", icon))
    }

    /// Repeatedly call provided async function until it succeeds.
    ///
    /// This function will call `f` in a loop. If it succeeds, the result will be returned.
    /// Otherwise, the block will enter error mode: "X" will be shown and on left click the error
    /// message will be shown.
    ///
    /// # Examples
    ///
    /// ```
    /// let status = api.recoverable(|| Status::new(&*socket_path)).await?;
    /// ```
    pub async fn recoverable<Fn, Fut, T>(&mut self, mut f: Fn) -> Result<T>
    where
        Fn: FnMut() -> Fut,
        Fut: Future<Output = Result<T>>,
    {
        loop {
            match f().await {
                Ok(res) => return Ok(res),
                Err(err) => {
                    self.set_error(err).await?;
                    tokio::select! {
                        _ = tokio::time::sleep(self.error_interval) => (),
                        _ = self.wait_for_update_request() => (),
                    }
                }
            }
        }
    }
}

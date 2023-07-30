//! The collection of blocks
//!
//! Blocks are defined as a [TOML array of tables](https://github.com/toml-lang/toml/blob/main/toml.md#user-content-array-of-tables): `[[block]]`
//!
//! Key | Description | Default
//! ----|-------------|----------
//! `block` | Name of the i3status-rs block you want to use. See `Blocks` below for valid block names. Must be the first field of a block config. | -
//! `signal` | Signal value that causes an update for this block with `0` corresponding to `-SIGRTMIN+0` and the largest value being `-SIGRTMAX` | None
//! `if_command` | Only display the block if the supplied command returns 0 on startup. | None
//! `merge_with_next` | If true this will group the block with the next one, so rendering such as alternating_tint will apply to the whole group | `false`
//! `icons_format` | Overrides global `icons_format` | None
//! `error_format` | Overrides global `error_format` | None
//! `error_fullscreen_format` | Overrides global `error_fullscreen_format` | None
//! `error_interval` | How long to wait until restarting the block after an error occurred. | `5`
//! `[block.theme_overrides]` | Same as the top-level config option, but for this block only. Refer to `Themes and Icons` below. | None
//! `[block.icons_overrides]` | Same as the top-level config option, but for this block only. Refer to `Themes and Icons` below. | None
//! `[[block.click]]` | Set or override click action for the block. See below for details. | Block default / None
//!
//! Per block click configuration `[[block.click]]`:
//!
//! Key | Description | Default
//! ----|-------------|----------
//! `button` | `left`, `middle`, `right`, `up`, `down`, `forward`, `back` or [`double_left`](https://greshake.github.io/i3status-rust/i3status_rs/click/enum.MouseButton.html). | -
//! `widget` | To which part of the block this entry applies | None
//! `cmd` | Command to run when the mouse button event is detected. | None
//! `action` | Which block action to trigger | None
//! `sync` | Whether to wait for command to exit or not. | `false`
//! `update` | Whether to update the block on click. | `false`

mod prelude;

use crate::BoxedFuture;
use futures::future::FutureExt;
use serde::de::{self, Deserialize};
use tokio::sync::{mpsc, Notify};

use std::borrow::Cow;
use std::sync::Arc;
use std::time::Duration;

use crate::click::MouseButton;
use crate::errors::*;
use crate::widget::Widget;
use crate::{Request, RequestCmd};

macro_rules! define_blocks {
    {
        $( $(#[cfg(feature = $feat: literal)])? $block: ident $(,)? )*
    } => {
        $(
            $(#[cfg(feature = $feat)])?
            $(#[cfg_attr(docsrs, doc(cfg(feature = $feat)))])?
            pub mod $block;
        )*

        #[derive(Debug)]
        pub enum BlockConfig {
            $(
                $(#[cfg(feature = $feat)])?
                #[allow(non_camel_case_types)]
                $block($block::Config),
            )*
            Err(Option<&'static str>, Error),
        }

        impl BlockConfig {
            pub fn name(&self) -> &'static str {
                match self {
                    $(
                        $(#[cfg(feature = $feat)])?
                        Self::$block { .. } => stringify!($block),
                    )*
                    Self::Err(Some(name), _err) => name,
                    Self::Err(None, _err) => "???",
                }
            }

            pub fn run(self, api: CommonApi) -> BlockFuture {
                let id = api.id;
                match self {
                    $(
                        $(#[cfg(feature = $feat)])?
                        Self::$block(config) => async move {
                            while let Err(err) = $block::run(&config, &api).await {
                                api.set_error(err).await?;
                                tokio::select! {
                                    _ = tokio::time::sleep(api.error_interval) => (),
                                    _ = api.wait_for_update_request() => (),
                                }
                            }
                            Ok(())
                        }.boxed_local(),
                    )*
                    Self::Err(name, err) => {
                        std::future::ready(Err(Error {
                            kind: ErrorKind::Config,
                            message: None,
                            cause: Some(Arc::new(err)),
                            block: Some((name.unwrap_or("???"), id)),
                        })).boxed_local()
                    },
                }
            }
        }

        impl<'de> Deserialize<'de> for BlockConfig {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: de::Deserializer<'de>,
            {
                use de::Error;

                let mut table = toml::Table::deserialize(deserializer)?;
                let block_name = table.remove("block").ok_or_else(|| D::Error::missing_field("block"))?;
                let block_name = block_name.as_str().ok_or_else(|| D::Error::custom("block must be a string"))?;

                match block_name {
                    $(
                        $(#[cfg(feature = $feat)])?
                        stringify!($block) => match $block::Config::deserialize(table) {
                            Ok(config) => Ok(BlockConfig::$block(config)),
                            Err(err) => Ok(BlockConfig::Err(Some(stringify!($block)), crate::errors::Error::new(err.to_string()))),
                        }
                        $(
                            #[cfg(not(feature = $feat))]
                            stringify!($block) => Ok(BlockConfig::Err(
                                Some(stringify!($block)),
                                crate::errors::Error::new(format!(
                                    "this block is behind a feature gate '{}' which must be enabled at compile time",
                                    $feat,
                                )),
                            )),
                        )?
                    )*
                    other => Err(D::Error::custom(format!("unknown block '{other}'")))
                }
            }
        }
    };
}

define_blocks!(
    amd_gpu,
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
    service_status,
    sound,
    speedtest,
    keyboard_layout,
    taskwarrior,
    temperature,
    time,
    tea_timer,
    toggle,
    uptime,
    vpn,
    watson,
    weather,
    xrandr,
);

pub type BlockFuture = BoxedFuture<Result<()>>;

pub type BlockAction = Cow<'static, str>;

#[derive(Clone)]
pub struct CommonApi {
    pub(crate) id: usize,
    pub(crate) update_request: Arc<Notify>,
    pub(crate) request_sender: mpsc::Sender<Request>,
    pub(crate) error_interval: Duration,
}

impl CommonApi {
    /// Sends the widget to be displayed.
    pub async fn set_widget(&self, widget: Widget) -> Result<()> {
        self.request_sender
            .send(Request {
                block_id: self.id,
                cmd: RequestCmd::SetWidget(widget),
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

    pub async fn set_default_actions(
        &self,
        actions: &'static [(MouseButton, Option<&'static str>, &'static str)],
    ) -> Result<()> {
        self.request_sender
            .send(Request {
                block_id: self.id,
                cmd: RequestCmd::SetDefaultActions(actions),
            })
            .await
            .error("Failed to send Request")
    }

    pub async fn get_actions(&self) -> Result<mpsc::UnboundedReceiver<BlockAction>> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.request_sender
            .send(Request {
                block_id: self.id,
                cmd: RequestCmd::SubscribeToActions(tx),
            })
            .await
            .error("Failed to send Request")?;
        Ok(rx)
    }

    pub async fn wait_for_update_request(&self) {
        self.update_request.notified().await;
    }
}

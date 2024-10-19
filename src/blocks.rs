//! The collection of blocks
//!
//! Blocks are defined as a [TOML array of tables](https://github.com/toml-lang/toml/blob/main/toml.md#user-content-array-of-tables): `[[block]]`
//!
//! Key | Description | Default
//! ----|-------------|----------
//! `block` | Name of the i3status-rs block you want to use. See [modules](#modules) below for valid block names. | -
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
//! `button` | `left`, `middle`, `right`, `up`, `down`, `forward`, `back` or [`double_left`](MouseButton). | -
//! `widget` | To which part of the block this entry applies (accepts regex) | `"block"`
//! `cmd` | Command to run when the mouse button event is detected. | None
//! `action` | Which block action to trigger | None
//! `sync` | Whether to wait for command to exit or not. | `false`
//! `update` | Whether to update the block on click. | `false`

mod prelude;

use futures::future::FutureExt;
use futures::stream::FuturesUnordered;
use serde::de::{self, Deserialize};
use tokio::sync::{mpsc, Notify};

use std::borrow::Cow;
use std::sync::Arc;
use std::time::Duration;

use crate::click::MouseButton;
use crate::errors::*;
use crate::widget::Widget;
use crate::{BoxedFuture, Request, RequestCmd};

macro_rules! define_blocks {
    {
        $(
            $(#[cfg(feature = $feat: literal)])?
            $(#[deprecated($($dep_k: ident = $dep_v: literal),+)])?
            $block: ident $(,)?
        )*
    } => {
        $(
            $(#[cfg(feature = $feat)])?
            $(#[cfg_attr(docsrs, doc(cfg(feature = $feat)))])?
            $(#[deprecated($($dep_k = $dep_v),+)])?
            pub mod $block;
        )*

        #[derive(Debug)]
        pub enum BlockConfig {
            $(
                $(#[cfg(feature = $feat)])?
                #[allow(non_camel_case_types)]
                #[allow(deprecated)]
                $block($block::Config),
            )*
            Err(&'static str, Error),
        }

        impl BlockConfig {
            pub fn name(&self) -> &'static str {
                match self {
                    $(
                        $(#[cfg(feature = $feat)])?
                        Self::$block { .. } => stringify!($block),
                    )*
                    Self::Err(name, _err) => name,
                }
            }

            pub fn spawn(self, api: CommonApi, futures: &mut FuturesUnordered<BoxedFuture<()>>) {
                match self {
                    $(
                        $(#[cfg(feature = $feat)])?
                        #[allow(deprecated)]
                        Self::$block(config) => futures.push(async move {
                            while let Err(err) = $block::run(&config, &api).await {
                                if api.set_error(err).is_err() {
                                    return;
                                }
                                tokio::select! {
                                    _ = tokio::time::sleep(api.error_interval) => (),
                                    _ = api.wait_for_update_request() => (),
                                }
                            }
                        }.boxed_local()),
                    )*
                    Self::Err(_name, err) => {
                        let _ = api.set_error(Error {
                            message: Some("Configuration error".into()),
                            cause: Some(Arc::new(err)),
                        });
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
                        #[allow(deprecated)]
                        stringify!($block) => match $block::Config::deserialize(table) {
                            Ok(config) => Ok(BlockConfig::$block(config)),
                            Err(err) => Ok(BlockConfig::Err(stringify!($block), crate::errors::Error::new(err.to_string()))),
                        }
                        $(
                            #[cfg(not(feature = $feat))]
                            stringify!($block) => Err(D::Error::custom(format!(
                                "block {} is behind a feature gate '{}' which must be enabled at compile time",
                                stringify!($block),
                                $feat,
                            ))),
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
    #[deprecated(
        since = "0.33.0",
        note = "The block has been deprecated in favor of the the packages block"
    )]
    apt,
    backlight,
    battery,
    bluetooth,
    calendar,
    cpu,
    custom,
    custom_dbus,
    disk_space,
    #[deprecated(
        since = "0.33.0",
        note = "The block has been deprecated in favor of the the packages block"
    )]
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
    packages,
    #[deprecated(
        since = "0.33.0",
        note = "The block has been deprecated in favor of the the packages block"
    )]
    pacman,
    pomodoro,
    privacy,
    rofication,
    service_status,
    scratchpad,
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

/// An error which originates from a block
#[derive(Debug, thiserror::Error)]
#[error("In block {}: {}", .block_name, .error)]
pub struct BlockError {
    pub block_id: usize,
    pub block_name: &'static str,
    pub error: Error,
}

pub type BlockAction = Cow<'static, str>;

#[derive(Clone)]
pub struct CommonApi {
    pub(crate) id: usize,
    pub(crate) update_request: Arc<Notify>,
    pub(crate) request_sender: mpsc::UnboundedSender<Request>,
    pub(crate) error_interval: Duration,
}

impl CommonApi {
    /// Sends the widget to be displayed.
    pub fn set_widget(&self, widget: Widget) -> Result<()> {
        self.request_sender
            .send(Request {
                block_id: self.id,
                cmd: RequestCmd::SetWidget(widget),
            })
            .error("Failed to send Request")
    }

    /// Hides the block. Send new widget to make it visible again.
    pub fn hide(&self) -> Result<()> {
        self.request_sender
            .send(Request {
                block_id: self.id,
                cmd: RequestCmd::UnsetWidget,
            })
            .error("Failed to send Request")
    }

    /// Sends the error to be displayed.
    pub fn set_error(&self, error: Error) -> Result<()> {
        self.request_sender
            .send(Request {
                block_id: self.id,
                cmd: RequestCmd::SetError(error),
            })
            .error("Failed to send Request")
    }

    pub fn set_default_actions(
        &self,
        actions: &'static [(MouseButton, Option<&'static str>, &'static str)],
    ) -> Result<()> {
        self.request_sender
            .send(Request {
                block_id: self.id,
                cmd: RequestCmd::SetDefaultActions(actions),
            })
            .error("Failed to send Request")
    }

    pub fn get_actions(&self) -> Result<mpsc::UnboundedReceiver<BlockAction>> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.request_sender
            .send(Request {
                block_id: self.id,
                cmd: RequestCmd::SubscribeToActions(tx),
            })
            .error("Failed to send Request")?;
        Ok(rx)
    }

    pub async fn wait_for_update_request(&self) {
        self.update_request.notified().await;
    }
}

#![warn(clippy::match_same_arms)]
#![warn(clippy::semicolon_if_nothing_returned)]
#![warn(clippy::unnecessary_wraps)]
#![allow(clippy::extra_unused_type_parameters)]

#[macro_use]
pub mod util;
pub mod blocks;
pub mod click;
pub mod config;
pub mod errors;
pub mod escape;
pub mod formatting;
pub mod icons;
pub mod netlink;
pub mod protocol;
pub mod signals;
pub mod subprocess;
pub mod themes;
pub mod widget;
pub mod wrappers;

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use clap::Parser;

use futures::Stream;

use once_cell::sync::Lazy;

use crate::click::MouseButton;
use crate::errors::Error;
use crate::protocol::i3bar_block::I3BarBlock;
use crate::widget::Widget;

const APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);
const REQWEST_TIMEOUT: Duration = Duration::from_secs(10);

pub static REQWEST_CLIENT: Lazy<reqwest::Client> = Lazy::new(|| {
    reqwest::Client::builder()
        .user_agent(APP_USER_AGENT)
        .timeout(REQWEST_TIMEOUT)
        .build()
        .unwrap()
});

pub static REQWEST_CLIENT_IPV4: Lazy<reqwest::Client> = Lazy::new(|| {
    reqwest::Client::builder()
        .user_agent(APP_USER_AGENT)
        .local_address(Some(std::net::Ipv4Addr::UNSPECIFIED.into()))
        .timeout(REQWEST_TIMEOUT)
        .build()
        .unwrap()
});

pub type BoxedFuture<T> = Pin<Box<dyn Future<Output = T>>>;

pub type BoxedStream<T> = Pin<Box<dyn Stream<Item = T>>>;

/// A feature-rich and resource-friendly replacement for i3status(1), written in Rust. The
/// i3status-rs program writes a stream of configurable "blocks" of system information (time,
/// battery status, volume, etc.) to standard output in the JSON format understood by i3bar(1) and
/// sway-bar(5).
#[derive(Debug, Parser)]
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

#[derive(Debug, Clone)]
pub struct RenderedBlock {
    pub segments: Vec<I3BarBlock>,
    pub merge_with_next: bool,
}

use clap::Parser;

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

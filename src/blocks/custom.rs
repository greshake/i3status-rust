//! The output of a custom shell command
//!
//! For further customisation, use the `json` option and have the shell command output valid JSON in the schema below:  
//! ```json
//! {"icon": "...", "state": "...", "text": "...", "short_text": "..."}
//! ```
//! `icon` is optional (default "")  
//! `state` is optional, it may be Idle, Info, Good, Warning, Critical (default Idle)  
//! `short_text` is optional.
//!
//! # Configuration
//!
//! Key | Values | Required | Default
//! ----|--------|----------|--------
//! `command` | Shell command to execute & display | No | None
//! `cycle` | Commands to execute and change when the button is clicked | No | None
//! `interval` | Update interval in seconds (or "once" to update only once) | No | `10`
//! `json` | Use JSON from command output to format the block. If the JSON is not valid, the block will error out. | No | `false`
//! `signal` | Signal value that causes an update for this block with 0 corresponding to `-SIGRTMIN+0` and the largest value being `-SIGRTMAX` | No | None
//! watch_files | Watch files to trigger update on file modification | No | None
//! `hide_when_empty` | Hides the block when the command output (or json text field) is empty | No | false
//! `shell` | Specify the shell to use when running commands | No | `$SHELL` if set, otherwise fallback to `sh`
//!
//! # Examples
//!
//! Display temperature, update every 10 seconds:
//!
//! ```toml
//! [[block]]
//! block = "custom"
//! command = ''' cat /sys/class/thermal/thermal_zone0/temp | awk '{printf("%.1f\n",$1/1000)}' '''
//! ```
//!
//! Cycle between "ON" and "OFF", update every 1 second, run `<command>` when block is clicked:
//!
//! ```toml
//! [[block]]
//! block = "custom"
//! cycle = ["echo ON", "echo OFF"]
//! interval = 1
//! [[block.click]]
//! button = "left"
//! cmd = "<command>"
//! ```
//!
//! Use JSON output:
//!
//! ```toml
//! [[block]]
//! block = "custom"
//! command = "echo '{\"icon\":\"weather_thunder\",\"state\":\"Critical\", \"text\": \"Danger!\"}'"
//! json = true
//! ```
//!
//! Display kernel, update the block only once:
//!
//! ```toml
//! [[block]]
//! block = "custom"
//! command = "uname -r"
//! interval = "once"
//! ```
//!
//! Display the screen brightness on an intel machine and update this only when `pkill -SIGRTMIN+4 i3status-rs` is called:
//!
//! ```toml
//! [[block]]
//! block = "custom"
//! command = ''' cat /sys/class/backlight/intel_backlight/brightness | awk '{print $1}' '''
//! signal = 4
//! interval = "once"
//! ```
//!
//! Update block when one or more specified files are modified:
//!
//! ```toml
//! [[block]]
//! block = "custom"
//! command = "cat custom_status"
//! watch_files = ["custom_status"]
//! interval = "once"
//! ```
//!
//! # TODO:
//! - Use `shellexpand`

use super::prelude::*;
use crate::signals::Signal;
use inotify::{Inotify, WatchMask};
use std::io;
use tokio::{process::Command, time::Instant};
use tokio_stream::wrappers::IntervalStream;

#[derive(Deserialize, Debug, Derivative)]
#[serde(deny_unknown_fields, default)]
#[derivative(Default)]
struct CustomConfig {
    command: Option<StdString>,
    cycle: Option<Vec<StdString>>,
    #[derivative(Default(value = "10.into()"))]
    interval: OnceDuration,
    json: bool,
    hide_when_empty: bool,
    shell: Option<StdString>,
    signal: Option<i32>,
    watch_files: Vec<StdString>,
}

pub async fn run(config: toml::Value, mut api: CommonApi) -> Result<()> {
    let mut events = api.get_events().await?;
    let config = CustomConfig::deserialize(config).config_error()?;

    // let mut notify;

    type TimerStream = Pin<Box<dyn Stream<Item = Instant> + Send + Sync>>;
    let mut timer: TimerStream = match config.interval {
        OnceDuration::Once => Box::pin(futures::stream::pending()),
        OnceDuration::Duration(dur) => Box::pin(IntervalStream::new(dur.timer())),
    };

    type FileStream = Pin<Box<dyn Stream<Item = io::Result<inotify::EventOwned>> + Send + Sync>>;
    let mut file_updates: FileStream = match config.watch_files.as_slice() {
        [] => Box::pin(futures::stream::pending()),
        files => {
            let mut notify = Inotify::init().error("Failed to start inotify")?;
            // TODO: is there a way to avoid this leak?
            let buf = Box::leak(Box::new([0; 1024]));

            for file in files {
                notify
                    .add_watch(file, WatchMask::MODIFY | WatchMask::CLOSE_WRITE)
                    .error("Failed to file")?;
            }
            Box::pin(
                notify
                    .event_stream(buf)
                    .error("Failed to create event stream")?,
            )
        }
    };

    // Choose the shell in this priority:
    // 1) `shell` config option
    // 2) `SHELL` environment varialble
    // 3) `"sh"`
    let shell = config
        .shell
        .or_else(|| std::env::var("SHELL").ok())
        .unwrap_or_else(|| "sh".to_string());

    let mut cycle = config
        .cycle
        .or_else(|| config.command.clone().map(|cmd| vec![cmd]))
        .error("either 'command' or 'cycle' must be specified")?
        .into_iter()
        .cycle();

    loop {
        // Run command
        let output = Command::new(&shell)
            .args(&["-c", &cycle.next().unwrap()])
            .output()
            .await
            .error("failed to run command")?;
        let stdout = std::str::from_utf8(&output.stdout)
            .error("the output of command is invalid UTF-8")?
            .trim();

        if stdout.is_empty() && config.hide_when_empty {
            api.hide();
        } else if config.json {
            let input: Input = serde_json::from_str(stdout).error("invalid JSON")?;

            api.show();
            api.set_icon(&input.icon)?;
            api.set_state(input.state);
            if let Some(short) = input.short_text {
                api.set_texts(input.text, short);
            } else {
                api.set_text(input.text);
            }
        } else {
            api.show();
            api.set_text(stdout.into());
        };
        api.flush().await?;

        if config.interval == OnceDuration::Once && config.watch_files.is_empty() {
            return Ok(());
        }

        loop {
            tokio::select! {
                _ = timer.next() => break,
                _ = file_updates.next() => break,
                Some(event) = events.recv() => {
                    match (event, config.signal) {
                        (BlockEvent::Signal(Signal::Custom(s)), Some(signal)) if s == signal => break,
                        (BlockEvent::Click(_), _) => break,
                        _ => (),
                    }
                },
            }
        }
    }
}

#[derive(Deserialize, Debug, Default)]
#[serde(default)]
struct Input {
    icon: String,
    state: State,
    text: String,
    short_text: Option<String>,
}

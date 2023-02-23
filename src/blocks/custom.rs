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
//! Key | Values | Default
//! ----|--------|--------
//! `format` | A string to customise the output of this block. See below for available placeholders. | <code>"{ $icon&vert;} $text.pango-str() "</code>
//! `command` | Shell command to execute & display | `None`
//! `persistent` | Run command in the background; update display for each output line of the command | `false`
//! `cycle` | Commands to execute and change when the button is clicked | `None`
//! `interval` | Update interval in seconds (or "once" to update only once) | `10`
//! `json` | Use JSON from command output to format the block. If the JSON is not valid, the block will error out. | `false`
//! `watch_files` | Watch files to trigger update on file modification | `None`
//! `hide_when_empty` | Hides the block when the command output (or json text field) is empty | `false`
//! `shell` | Specify the shell to use when running commands | `$SHELL` if set, otherwise fallback to `sh`
//!
//! Placeholder      | Value                                                      | Type   | Unit
//! -----------------|------------------------------------------------------------|--------|---------------
//! `icon`           | Value of icon field from JSON output when it's non-empty   | Icon   | -
//! `text`           | Output of the script or text field from JSON output        | Text   |
//! `short_text`     | short_text field from JSON output                          | Text   |
//!
//! Action  | Default button
//! --------|---------------
//! `cycle` | Left
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
use inotify::{Inotify, WatchMask};
use std::process::Stdio;
use tokio::io::{self, AsyncBufReadExt, BufReader};
use tokio::process::Command;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(default)]
pub struct Config {
    format: FormatConfig,
    command: Option<String>,
    persistent: bool,
    cycle: Option<Vec<String>>,
    #[default(10.into())]
    interval: Seconds,
    json: bool,
    hide_when_empty: bool,
    shell: Option<String>,
    watch_files: Vec<String>,
}

async fn update_bar(
    stdout: &str,
    hide_when_empty: bool,
    json: bool,
    api: &mut CommonApi,
    widget: &mut Widget,
) -> Result<()> {
    let text_empty;

    if json {
        match serde_json::from_str::<Input>(stdout).error("Invalid JSON") {
            Ok(input) => {
                text_empty = input.text.is_empty();
                widget.set_values(map! {
                    "text" => Value::text(input.text),
                    [if !input.icon.is_empty()] "icon" => Value::icon(api.get_icon(&input.icon)?),
                    [if let Some(t) = input.short_text] "short_text" => Value::text(t)
                });
                widget.state = input.state;
            }
            Err(error) => return api.set_error(error).await,
        }
    } else {
        text_empty = stdout.is_empty();
        widget.set_values(map!("text" => Value::text(stdout.into())));
    }

    if text_empty && hide_when_empty {
        api.hide().await
    } else {
        api.set_widget(widget).await
    }
}

pub async fn run(config: Config, mut api: CommonApi) -> Result<()> {
    api.set_default_actions(&[(MouseButton::Left, None, "cycle")])
        .await?;

    let mut widget = Widget::new().with_format(config.format.with_defaults(
        "{ $icon|} $text.pango-str() ",
        "{ $icon|} $short_text.pango-str() |",
    )?);

    let mut timer = config.interval.timer();

    type FileStream = Pin<Box<dyn Stream<Item = io::Result<inotify::EventOwned>> + Send + Sync>>;
    let mut file_updates: FileStream = match config.watch_files.as_slice() {
        [] => Box::pin(futures::stream::pending()),
        files => {
            let mut notify = Inotify::init().error("Failed to start inotify")?;
            for file in files {
                notify
                    .add_watch(file, WatchMask::MODIFY | WatchMask::CLOSE_WRITE)
                    .error("Failed to file")?;
            }
            Box::pin(
                notify
                    .event_stream([0; 1024])
                    .error("Failed to create event stream")?,
            )
        }
    };

    // Choose the shell in this priority:
    // 1) `shell` config option
    // 2) `SHELL` environment variable
    // 3) `"sh"`
    let shell = config
        .shell
        .or_else(|| std::env::var("SHELL").ok())
        .unwrap_or_else(|| "sh".to_string());

    if config.persistent {
        let mut process = Command::new(&shell)
            .args([
                "-c",
                config
                    .command
                    .as_deref()
                    .error("'command' must be specified when 'persistent' is set")?,
            ])
            .stdout(Stdio::piped())
            .spawn()
            .error("failed to run command")?;

        let stdout = process
            .stdout
            .take()
            .expect("child did not have a handle to stdout");
        let mut reader = BufReader::new(stdout).lines();

        tokio::spawn(async move {
            let _ = process.wait().await;
        });

        loop {
            select! {
                line = reader.next_line() => {
                    let line = line.error("error reading line from child process")?.error("child process exited unexpectedly")?;
                    update_bar(&line, config.hide_when_empty, config.json, &mut api, &mut widget).await?;
                }
                // events must be polled
                _ = api.event() => (),
            }
        }
    } else {
        let mut cycle = config
            .cycle
            .or_else(|| config.command.clone().map(|cmd| vec![cmd]))
            .error("either 'command' or 'cycle' must be specified")?
            .into_iter()
            .cycle();
        let mut cmd = cycle.next().unwrap();

        loop {
            // Run command
            let output = Command::new(&shell)
                .args(["-c", &cmd])
                .output()
                .await
                .error("failed to run command")?;
            let stdout = std::str::from_utf8(&output.stdout)
                .error("the output of command is invalid UTF-8")?
                .trim();

            update_bar(
                stdout,
                config.hide_when_empty,
                config.json,
                &mut api,
                &mut widget,
            )
            .await?;

            loop {
                select! {
                    _ = timer.tick() => break,
                    _ = file_updates.next() => break,
                    event = api.event() => match event {
                        UpdateRequest => break,
                        Action(a) if a == "cycle" => {
                            cmd = cycle.next().unwrap();
                            break;
                        }
                        _ => (),
                    }
                }
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

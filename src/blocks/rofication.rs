//! The number of pending notifications in rofication-daemon
//!
//! A different color is used is there are critical notications. Left clicking the block opens the GUI.
//!
//! # Configuration
//!
//! Key | Values | Required | Default
//! ----|--------|----------|--------
//! `interval` | Refresh rate in seconds. | No | `1`
//! `format` | A string to customise the output of this block. See below for placeholders. | No | `"$num.eng(1)"`
//! `socket_path` | Socket path for the rofication daemon. | No | "/tmp/rofi_notification_daemon"
//!
//!  Key | Value | Type | Unit
//! -----|-------|------|-----
//! `num` | Number of pending notifications | Number | -
//!
//! # Example
//!
//! ```toml
//! [[block]]
//! block = "rofication"
//! interval = 1
//! socket_path = "/tmp/rofi_notification_daemon"
//! ```
//!
//! # Icons Used
//! - `bell`

use tokio::net::UnixStream;

use super::prelude::*;
use crate::subprocess::spawn_shell;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
struct RoficationConfig {
    #[default(1.into())]
    interval: Seconds,
    #[default("/tmp/rofi_notification_daemon".into())]
    socket_path: ShellString,
    format: FormatConfig,
}

pub async fn run(config: toml::Value, mut api: CommonApi) -> Result<()> {
    let config = RoficationConfig::deserialize(config).config_error()?;
    api.set_format(config.format.with_default("$num.eng(1)")?);
    api.set_icon("bell")?;

    let path = config.socket_path.expand()?;
    let mut timer = config.interval.timer();

    loop {
        let (num, crit) = api.recoverable(|| rofication_status(&path), "X").await?;

        api.set_values(map!("num" => Value::number(num)));
        api.set_state(if crit > 0 {
            State::Warning
        } else if num > 0 {
            State::Info
        } else {
            State::Idle
        });

        api.flush().await?;

        loop {
            tokio::select! {
                _ = timer.tick() => break,
                event = api.event() => match event {
                    UpdateRequest => break,
                    Click(click) => {
                        if click.button == MouseButton::Left {
                            let _ = spawn_shell("rofication-gui");
                            break;
                        }
                    }
                }
            }
        }
    }
}

async fn rofication_status(socket_path: &str) -> Result<(usize, usize)> {
    let mut stream = UnixStream::connect(socket_path)
        .await
        .error("Failed to connect to socket")?;

    // Request count
    stream
        .write_all(b"num")
        .await
        .error("Failed to write to socket")?;

    let mut responce = String::new();
    stream
        .read_to_string(&mut responce)
        .await
        .error("Failed to read from socket")?;

    // Response must be two integers: regular and critical, separated eihter by a comma or a \n
    let mut parts = responce.split(|x| x == ',' || x == '\n');
    let num = parts
        .next()
        .and_then(|x| x.parse::<usize>().ok())
        .error("Incorrect responce")?;
    let crit = parts
        .next()
        .and_then(|x| x.parse::<usize>().ok())
        .error("Incorrect responce")?;

    if parts.next().is_some() {
        Err(Error::new("Incorrect responce"))
    } else {
        Ok((num, crit))
    }
}

//! The number of pending notifications in rofication-daemon
//!
//! A different color is used is there are critical notifications.
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `interval` | Refresh rate in seconds. | `1`
//! `format` | A string to customise the output of this block. See below for placeholders. | `" $icon $num.eng(w:1) "`
//! `socket_path` | Socket path for the rofication daemon. Supports path expansions e.g. `~`. | `"/tmp/rofi_notification_daemon"`
//!
//!  Placeholder | Value | Type | Unit
//! -------------|-------|------|-----
//! `icon`       | A static icon  | Icon | -
//! `num`        | Number of pending notifications | Number | -
//!
//! # Example
//!
//! ```toml
//! [[block]]
//! block = "rofication"
//! interval = 1
//! socket_path = "/tmp/rofi_notification_daemon"
//! [[block.click]]
//! button = "left"
//! cmd = "rofication-gui"
//! ```
//!
//! # Icons Used
//! - `bell`

use super::prelude::*;
use tokio::net::UnixStream;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(default)]
pub struct Config {
    #[default(1.into())]
    interval: Seconds,
    #[default("/tmp/rofi_notification_daemon".into())]
    socket_path: ShellString,
    format: FormatConfig,
}

pub async fn run(config: Config, mut api: CommonApi) -> Result<()> {
    let mut widget =
        Widget::new().with_format(config.format.with_default(" $icon $num.eng(w:1) ")?);

    let path = config.socket_path.expand()?;
    let mut timer = config.interval.timer();

    loop {
        let (num, crit) = api.recoverable(|| rofication_status(&path)).await?;

        widget.set_values(map!(
            "icon" => Value::icon(api.get_icon("bell")?),
            "num" => Value::number(num)
        ));

        widget.state = if crit > 0 {
            State::Warning
        } else if num > 0 {
            State::Info
        } else {
            State::Idle
        };
        api.set_widget(&widget).await?;

        tokio::select! {
            _ = timer.tick() => (),
            _ = api.wait_for_update_request() => (),
        }
    }
}

async fn rofication_status(socket_path: &str) -> Result<(usize, usize)> {
    let mut stream = UnixStream::connect(socket_path)
        .await
        .error("Failed to connect to socket")?;

    // Request count
    stream
        .write_all(b"num:\n")
        .await
        .error("Failed to write to socket")?;

    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .await
        .error("Failed to read from socket")?;

    // Response must be two integers: regular and critical, separated either by a comma or a \n
    let (num, crit) = response
        .split_once(|x| x == ',' || x == '\n')
        .error("Incorrect response")?;
    Ok((
        num.parse().error("Incorrect response")?,
        crit.parse().error("Incorrect response")?,
    ))
}

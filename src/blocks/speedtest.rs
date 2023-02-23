//! Ping, download, and upload speeds
//!
//! This block which requires [`speedtest-cli`](https://github.com/sivel/speedtest-cli).
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `format` | A string to customise the output of this block. See below for available placeholders. | `" ^icon_ping $ping ^icon_net_down $speed_down ^icon_net_up $speed_up "`
//! `interval` | Update interval in seconds | `1800`
//!
//! Placeholder  | Value          | Type   | Unit
//! -------------|----------------|--------|---------------
//! `ping`       | Ping delay     | Number | Seconds
//! `speed_down` | Download speed | Number | Bits per second
//! `speed_up`   | Upload speed   | Number | Bits per second
//!
//! # Example
//!
//! Show only ping (with an icon)
//!
//! ```toml
//! [[block]]
//! block = "speedtest"
//! interval = 1800
//! format = " ^icon_ping $ping "
//! ```
//!
//! Hide ping and display speed in bytes per second each using 4 characters (without icons)
//!
//! ```toml
//! [[block]]
//! block = "speedtest"
//! interval = 1800
//! format = " $speed_down.eng(w:4,u:B) $speed_up(w:4,u:B) "
//! ```
//!
//! # Icons Used
//! - `ping`
//! - `net_down`
//! - `net_up`

use super::prelude::*;
use tokio::process::Command;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(default)]
pub struct Config {
    format: FormatConfig,
    #[default(1800.into())]
    interval: Seconds,
}

pub async fn run(config: Config, mut api: CommonApi) -> Result<()> {
    let mut widget =
        Widget::new().with_format(config.format.with_default(
            " ^icon_ping $ping ^icon_net_down $speed_down ^icon_net_up $speed_up ",
        )?);

    let mut command = Command::new("speedtest-cli");
    command.arg("--json");

    loop {
        let output = command
            .output()
            .await
            .error("failed to run 'speedtest-cli'")?
            .stdout;
        let output =
            std::str::from_utf8(&output).error("'speedtest-cli' produced non-UTF8 output")?;
        let output: SpeedtestCliOutput =
            serde_json::from_str(output).error("'speedtest-cli' produced wrong JSON")?;

        widget.set_values(map! {
            "ping" => Value::seconds(output.ping * 1e-3),
            "speed_down" => Value::bits(output.download),
            "speed_up" => Value::bits(output.upload),
        });
        api.set_widget(&widget).await?;

        select! {
            _ = sleep(config.interval.0) => (),
            _ = api.wait_for_update_request() => (),
        }
    }
}

#[derive(Deserialize, Debug, Clone, Copy)]
struct SpeedtestCliOutput {
    /// Download speed in bits per second
    download: f64,
    /// Upload speed in bits per second
    upload: f64,
    /// Ping time in ms
    ping: f64,
}

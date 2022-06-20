//! Ping, download, and upload speeds
//!
//! This block which requires [`speedtest-cli`](https://github.com/sivel/speedtest-cli).
//!
//! # Configuration
//!
//! Key | Values | Required | Default
//! ----|--------|----------|--------
//! `format` | A string to customise the output of this block. See below for available placeholders. | No | `"$ping$speed_down$speed_up"`
//! `interval` | Update interval in seconds | No | `1800`
//!
//! Placeholder  | Value          | Type   | Unit
//! -------------|----------------|--------|---------------
//! `ping`       | Ping delay     | Number | Seconds
//! `speed_down` | Download speed | Number | Bits per second
//! `speed_up`   | Upload speed   | Number | Bits per second
//!
//! # Example
//!
//! Hide ping and display speed in bytes per second each using 4 characters
//!
//! ```toml
//! [[block]]
//! block = "speedtest"
//! interval = 1800
//! format = "$speed_down.eng(4,B)$speed_up(4,B)"
//! ```
//!
//! # Icons Used
//! - `ping`
//! - `net_down`
//! - `net_up`

use super::prelude::*;
use tokio::process::Command;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
struct SpeedtestConfig {
    format: FormatConfig,
    #[default(1800.into())]
    interval: Seconds,
}

pub async fn run(config: toml::Value, mut api: CommonApi) -> Result<()> {
    let config = SpeedtestConfig::deserialize(config).config_error()?;
    let mut widget = api
        .new_widget()
        .with_format(config.format.with_default("$ping$speed_down$speed_up")?);

    let icon_ping = api.get_icon("ping")?;
    let icon_down = api.get_icon("net_down")?;
    let icon_up = api.get_icon("net_up")?;

    let mut command = Command::new("speedtest-cli");
    command.arg("--json");

    loop {
        let output = command
            .output()
            .await
            .error("failed to run 'speedtest-cli'")?
            .stdout;
        let output =
            std::str::from_utf8(&output).error("'speedtest-cli' produced non-UTF8 outupt")?;
        let output: SpeedtestCliOutput =
            serde_json::from_str(output).error("'speedtest-cli' produced wrong JSON")?;

        widget.set_values(map! {
            "ping" => Value::seconds(output.ping * 1e-3).with_icon(icon_ping.clone()),
            "speed_down" => Value::bits(output.download).with_icon(icon_down.clone()),
            "speed_up" => Value::bits(output.upload).with_icon(icon_up.clone()),
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

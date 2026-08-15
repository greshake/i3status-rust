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
//! - `ping` (`^icon_ping`)
//! - `net_down` (`^icon_net_down`)
//! - `net_up` (`^icon_net_up`)

use super::prelude::*;
use tokio::process::Command;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub format: FormatConfig,
    #[default(1800.into())]
    pub interval: Seconds,
}

pub(crate) fn prepare(config: &Config) -> Result<Arc<BlockPlan>> {
    // The icons (`ping`, `net_down`, `net_up`) are rendered by `^icon_*`
    // format tokens, not icon-valued placeholders, so no icons are declared.
    BlockPlan::new(vec![OutputPlan::new(
        "main",
        config
            .format
            .with_default(" ^icon_ping $ping ^icon_net_down $speed_down ^icon_net_up $speed_up ")?,
    )])
}

pub(crate) async fn run(config: &Config, api: &CommonApi, plan: &Arc<BlockPlan>) -> Result<()> {
    let output_main = plan.output("main")?;

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

        let mut widget = output_main.new_widget();
        widget.set_values(map! {
            "ping" => Value::seconds(output.ping * 1e-3),
            "speed_down" => Value::bits(output.download),
            "speed_up" => Value::bits(output.upload),
        });
        api.set_widget(widget)?;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_declares_main_output_without_icon_placeholders() {
        let plan = prepare(&Config::default()).unwrap();
        let declared: Vec<_> = plan.outputs().map(|o| o.id()).collect();
        assert_eq!(declared, ["main"]);
        let output = plan.output("main").unwrap();
        // Every icon this block draws is a `^icon_*` token in the format
        // rather than an icon value, so the icon surface is entirely static.
        assert_eq!(output.output().icon_placeholders().count(), 0);
        assert_eq!(
            output.output().static_icons(),
            ["ping", "net_down", "net_up"]
        );
    }

    #[test]
    fn a_format_without_icons_declares_none() {
        let config = Config {
            format: " $ping ".parse().unwrap(),
            ..Config::default()
        };
        let plan = prepare(&config).unwrap();
        let output = plan.output("main").unwrap();
        assert!(output.output().static_icons().is_empty());
    }

    #[test]
    fn custom_format_is_respected() {
        let config = Config {
            format: " $ping ".parse().unwrap(),
            ..Config::default()
        };
        let plan = prepare(&config).unwrap();
        let output = plan.output("main").unwrap();
        assert!(output.format().contains_key("ping"));
        assert!(!output.format().contains_key("speed_down"));
    }
}

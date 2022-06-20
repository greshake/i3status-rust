//! System load average
//!
//! # Configuration
//!
//! Key        | Values                                                                                | Required | Default
//! -----------|---------------------------------------------------------------------------------------|----------|--------
//! `format`   | A string to customise the output of this block. See below for available placeholders. | No       | `"$1m"`
//! `interval` | Update interval in seconds                                                            | No       | `3`
//! `info`     | Minimum load, where state is set to info                                              | No       | `0.3`
//! `warning`  | Minimum load, where state is set to warning                                           | No       | `0.6`
//! `critical` | Minimum load, where state is set to critical                                          | No       | `0.9`
//!
//! Placeholder  | Value                  | Type   | Unit
//! -------------|------------------------|--------|-----
//! `1m`         | 1 minute load average  | Number | -
//! `5m`         | 5 minute load average  | Number | -
//! `15m`        | 15 minute load average | Number | -
//!
//! # Example
//!
//! ```toml
//! [[block]]
//! block = "load"
//! format = "1min avg: $1m"
//! interval = 1
//! ```
//!
//! # Icons Used
//! - `cogs`

use super::prelude::*;
use crate::util;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
struct LoadConfig {
    format: FormatConfig,
    #[default(3.into())]
    interval: Seconds,
    #[default(0.3)]
    info: f64,
    #[default(0.6)]
    warning: f64,
    #[default(0.9)]
    critical: f64,
}

pub async fn run(config: toml::Value, mut api: CommonApi) -> Result<()> {
    let config = LoadConfig::deserialize(config).config_error()?;
    let mut widget = api
        .new_widget()
        .with_icon("cogs")?
        .with_format(config.format.with_default("$1m")?);

    // borrowed from https://docs.rs/cpuinfo/0.1.1/src/cpuinfo/count/logical.rs.html#4-6
    let logical_cores = util::read_file("/proc/cpuinfo")
        .await
        .error("Your system doesn't support /proc/cpuinfo")?
        .lines()
        .filter(|l| l.starts_with("processor"))
        .count();

    loop {
        let loadavg = util::read_file("/proc/loadavg")
            .await
            .error("Your system does not support reading the load average from /proc/loadavg")?;
        let mut values = loadavg.split(' ');
        let m1: f64 = values
            .next()
            .and_then(|x| x.parse().ok())
            .error("bad /proc/loadavg file")?;
        let m5: f64 = values
            .next()
            .and_then(|x| x.parse().ok())
            .error("bad /proc/loadavg file")?;
        let m15: f64 = values
            .next()
            .and_then(|x| x.parse().ok())
            .error("bad /proc/loadavg file")?;

        widget.state = match m1 / logical_cores as f64 {
            x if x > config.critical => State::Critical,
            x if x > config.warning => State::Warning,
            x if x > config.info => State::Info,
            _ => State::Idle,
        };
        widget.set_values(map! {
            "1m" => Value::number(m1),
            "5m" => Value::number(m5),
            "15m" => Value::number(m15),
        });
        api.set_widget(&widget).await?;

        select! {
            _ = sleep(config.interval.0) => (),
            _ = api.wait_for_update_request() => (),
        }
    }
}

//! System load average
//!
//! # Configuration
//!
//! Key        | Values                                                                                | Default
//! -----------|---------------------------------------------------------------------------------------|--------
//! `format`   | A string to customise the output of this block. See below for available placeholders. | `" $icon $1m.eng(w:4) "`
//! `interval` | Update interval in seconds                                                            | `3`
//! `info`     | Minimum load, where state is set to info                                              | `0.3`
//! `warning`  | Minimum load, where state is set to warning                                           | `0.6`
//! `critical` | Minimum load, where state is set to critical                                          | `0.9`
//!
//! Placeholder  | Value                  | Type   | Unit
//! -------------|------------------------|--------|-----
//! `icon`       | A static icon          | Icon   | -
//! `1m`         | 1 minute load average  | Number | -
//! `5m`         | 5 minute load average  | Number | -
//! `15m`        | 15 minute load average | Number | -
//!
//! # Example
//!
//! ```toml
//! [[block]]
//! block = "load"
//! format = " $icon 1min avg: $1m.eng(w:4) "
//! interval = 1
//! ```
//!
//! # Icons Used
//! - `cogs`

use super::prelude::*;
use crate::util;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub format: FormatConfig,
    #[default(3.into())]
    pub interval: Seconds,
    #[default(0.3)]
    pub info: f64,
    #[default(0.6)]
    pub warning: f64,
    #[default(0.9)]
    pub critical: f64,
}

pub(crate) fn prepare(config: &Config) -> Result<Arc<BlockPlan>> {
    BlockPlan::new(vec![
        OutputPlan::new("main", config.format.with_default(" $icon $1m.eng(w:4) ")?)
            .icon("icon", IconChoices::one("cogs")),
    ])
}

pub async fn run(config: &Config, api: &CommonApi, plan: &Arc<BlockPlan>) -> Result<()> {
    let output_main = plan.output("main")?;

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

        let mut widget = output_main.new_widget();
        widget.state = match m1 / logical_cores as f64 {
            x if x > config.critical => State::Critical,
            x if x > config.warning => State::Warning,
            x if x > config.info => State::Info,
            _ => State::Idle,
        };
        widget.set_values(map! {
            "icon" => output_main.icon_value("icon")?,
            "1m" => Value::number(m1),
            "5m" => Value::number(m5),
            "15m" => Value::number(m15),
        });
        api.set_widget(widget)?;

        select! {
            _ = sleep(config.interval.0) => (),
            _ = api.wait_for_update_request() => (),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_declares_main_output_with_cogs_icon() {
        let plan = prepare(&Config::default()).unwrap();
        let declared: Vec<_> = plan.outputs.iter().map(|o| o.id).collect();
        assert_eq!(declared, ["main"]);
        let output = plan.output("main").unwrap();
        assert_eq!(output.single_icon("icon").unwrap(), "cogs");
    }


    #[test]
    fn custom_format_is_respected() {
        let config = Config {
            format: " $5m ".parse().unwrap(),
            ..Config::default()
        };
        let plan = prepare(&config).unwrap();
        let output = plan.output("main").unwrap();
        assert!(output.format().contains_key("5m"));
        assert!(!output.format().contains_key("icon"));
    }
}

//! System's uptime
//!
//! This block displays system uptime in terms of two biggest units, so minutes and seconds, or
//! hours and minutes or days and hours or weeks and days.
//!
//! # Configuration
//!
//! Key        | Values                     | Default
//! -----------|----------------------------|--------
//! `format` | A string to customise the output of this block. See below for available placeholders | `" $icon $uptime "`
//! `interval` | Update interval in seconds | `60`
//!
//! Placeholder         | Value                   | Type     | Unit
//! --------------------|-------------------------|----------|-----
//! `icon`              | A static icon           | Icon     | -
//! `text` *DEPRECATED* | Current uptime          | Text     | -
//! `uptime`            | Current uptime          | Duration | -
//!
//! `text` has been deprecated in favor of `uptime`.
//!
//! # Example
//!
//! ```toml
//! [[block]]
//! block = "uptime"
//! interval = 3600 # update every hour
//! ```
//!
//! # Used Icons
//! - `uptime`

use super::prelude::*;
use tokio::fs::read_to_string;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub format: FormatConfig,
    #[default(60.into())]
    pub interval: Seconds,
}

pub async fn run(config: &Config, api: &CommonApi) -> Result<()> {
    let format = config.format.with_default(" $icon $uptime ")?;

    loop {
        let uptime = read_to_string("/proc/uptime")
            .await
            .error("Failed to read /proc/uptime")?;
        let mut seconds: u64 = uptime
            .split('.')
            .next()
            .and_then(|u| u.parse().ok())
            .error("/proc/uptime has invalid content")?;

        let uptime = Duration::from_secs(seconds);

        let weeks = seconds / 604_800;
        seconds %= 604_800;
        let days = seconds / 86_400;
        seconds %= 86_400;
        let hours = seconds / 3_600;
        seconds %= 3_600;
        let minutes = seconds / 60;
        seconds %= 60;

        let text = if weeks > 0 {
            format!("{weeks}w {days}d")
        } else if days > 0 {
            format!("{days}d {hours}h")
        } else if hours > 0 {
            format!("{hours}h {minutes}m")
        } else {
            format!("{minutes}m {seconds}s")
        };

        let mut widget = Widget::new().with_format(format.clone());
        widget.set_values(map! {
          "icon" => Value::icon("uptime"),
          "text" => Value::text(text),
          "uptime" => Value::duration(uptime)
        });
        api.set_widget(widget)?;

        select! {
            _ = sleep(config.interval.0) => (),
            _ = api.wait_for_update_request() => (),
        }
    }
}

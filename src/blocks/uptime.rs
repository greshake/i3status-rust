//! System's uptime
//!
//! This block displays system uptime in terms of two biggest units, so minutes and seconds, or
//! hours and minutes or days and hours or weeks and days.
//!
//! # Configuration
//!
//! Key        | Values                     | Default
//! -----------|----------------------------|--------
//! `interval` | Update interval in seconds | `60`
//!
//! # Example
//!
//! ```toml
//! [[block]]
//! block = "uptime"
//! interval = "3600" # update every hour
//! ```
//!
//! # Used Icons
//! - `uptime`
//!
//! # TODO:
//! - Add `time` or `dur` formatter to `src/formatting/formatter.rs`

use super::prelude::*;
use tokio::fs::read_to_string;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
struct UptimeConfig {
    #[default(60.into())]
    interval: Seconds,
}

pub async fn run(config: toml::Value, mut api: CommonApi) -> Result<()> {
    let config = UptimeConfig::deserialize(config).config_error()?;
    let mut widget = api.new_widget().with_icon("uptime")?;

    loop {
        let uptime = read_to_string("/proc/uptime")
            .await
            .error("Failed to read /proc/uptime")?;
        let mut seconds: u64 = uptime
            .split('.')
            .next()
            .and_then(|u| u.parse().ok())
            .error("/proc/uptime has invalid content")?;

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

        widget.set_text(text);
        api.set_widget(&widget).await?;

        select! {
            _ = sleep(config.interval.0) => (),
            _ = api.wait_for_update_request() => (),
        }
    }
}

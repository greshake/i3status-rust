//! Network information
//!
//! This block uses `sysfs` and `netlink` and thus does not require any external dependencies.
//!
//! # Configuration
//!
//! Key | Values | Required | Default
//! ----|--------|----------|--------
//! `format` | A string to customise the output of this block. See below for available placeholders. | No | `"$speed_down.eng(3,B,K)$speed_up.eng(3,B,K)"`
//! `format_alt` | If set, block will switch between `format` and `format_alt` on every click | No | None
//! `device` | Network interface to monitor (as specified in `/sys/class/net/`) | No | If not set, device will be automatically selected every `interval`
//! `interval` | Update interval in seconds | No | `2`
//!
//! Placeholder  | Value                    | Type   | Unit
//! -------------|--------------------------|--------|---------------
//! `speed_down` | Download speed           | Number | Bytes per second
//! `speed_up`   | Upload speed             | Number | Bytes per second
//! `graph_down` | Download speed graph     | Text   | -
//! `graph_up`   | Upload speed graph       | Text   | -
//! `device`     | The name of device       | Text   | -
//! `ssid`       | Netfork SSID (WiFi only) | Text   | -
//! `frequency`  | WiFi frequency           | Number | Hz
//! `signal`     | WiFi signal              | Number | %
//!
//! # Example
//!
//! Display WiFi info if available
//!
//! ```toml
//! [[block]]
//! block = "net"
//! format = "{$signal.eng(2) $ssid.str() $frequency.eng()|Wired connection} via $device.str()"
//! ```
//!
//! # Icons Used
//! - `net_loopback`
//! - `net_vpn`
//! - `net_wired`
//! - `net_wireless`
//! - `net_up`
//! - `net_down`

use super::prelude::*;
use crate::netlink::{default_interface, NetDevice};
use crate::util;
use std::time::Instant;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
struct NetConfig {
    #[default(2.into())]
    interval: Seconds,
    format: FormatConfig,
    format_alt: Option<FormatConfig>,
    device: Option<String>,
}

pub async fn run(config: toml::Value, mut api: CommonApi) -> Result<()> {
    let config = NetConfig::deserialize(config).config_error()?;
    let mut format = config
        .format
        .with_default("$speed_down.eng(3,B,K)$speed_up.eng(3,B,K)")?;
    let mut format_alt = match config.format_alt {
        Some(f) => Some(f.with_default("")?),
        None => None,
    };
    api.set_format(format.clone());

    let mut timer = config.interval.timer();

    // Stats
    let mut stats = None;
    let mut stats_timer = Instant::now();
    let mut tx_hist = [0f64; 8];
    let mut rx_hist = [0f64; 8];

    loop {
        let mut speed_down: f64 = 0.0;
        let mut speed_up: f64 = 0.0;

        // Get interface name
        let device = NetDevice::from_interface(match &config.device {
            Some(i) => i.clone(),
            None => default_interface().await.unwrap_or_else(|| "lo".into()),
        })
        .await;

        // Calculate speed
        match (stats, device.read_stats().await) {
            // No previous stats available
            (None, new_stats) => stats = new_stats,
            // No new stats available
            (Some(_), None) => stats = None,
            // All stats available
            (Some(old_stats), Some(new_stats)) => {
                let diff = new_stats - old_stats;
                let elapsed = stats_timer.elapsed().as_secs_f64();
                stats_timer = Instant::now();
                speed_down = diff.rx_bytes as f64 / elapsed;
                speed_up = diff.tx_bytes as f64 / elapsed;
                stats = Some(new_stats);
            }
        }
        push_to_hist(&mut rx_hist, speed_down);
        push_to_hist(&mut tx_hist, speed_up);

        let wifi = device.wifi_info().await?;

        let mut values = map! {
            "speed_down" => Value::bytes(speed_down).with_icon(api.get_icon("net_down")?),
            "speed_up" => Value::bytes(speed_up).with_icon(api.get_icon("net_up")?),
            "graph_down" => Value::text(util::format_bar_graph(&rx_hist)),
            "graph_up" => Value::text(util::format_bar_graph(&tx_hist)),
            "device" => Value::text(device.interface),
        };
        wifi.0
            .map(|s| values.insert("ssid".into(), Value::text(s)));
        wifi.1
            .map(|f| values.insert("frequency".into(), Value::hertz(f)));
        wifi.2
            .map(|s| values.insert("signal".into(), Value::percents(s)));

        api.set_values(values);
        api.set_icon(device.icon)?;
        api.flush().await?;

        loop {
            select! {
                _ = timer.tick() => break,
                event = api.event() => match event {
                    UpdateRequest => break,
                    Click(click) => {
                        if click.button == MouseButton::Left {
                            if let Some(ref mut format_alt) = format_alt {
                                std::mem::swap(format_alt, &mut format);
                                api.set_format(format.clone());
                                break;
                            }
                        }
                    }
                }
            }
        }
    }
}

fn push_to_hist<T>(hist: &mut [T], elem: T) {
    hist[0] = elem;
    hist.rotate_left(1);
}

#[cfg(test)]
mod tests {
    use super::push_to_hist;

    #[test]
    fn test_push_to_hist() {
        let mut hist = [0; 4];
        assert_eq!(&hist, &[0, 0, 0, 0]);
        push_to_hist(&mut hist, 1);
        assert_eq!(&hist, &[0, 0, 0, 1]);
        push_to_hist(&mut hist, 3);
        assert_eq!(&hist, &[0, 0, 1, 3]);
        push_to_hist(&mut hist, 0);
        assert_eq!(&hist, &[0, 1, 3, 0]);
        push_to_hist(&mut hist, 10);
        assert_eq!(&hist, &[1, 3, 0, 10]);
        push_to_hist(&mut hist, 2);
        assert_eq!(&hist, &[3, 0, 10, 2]);
    }
}

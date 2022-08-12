//! Network information
//!
//! This block uses `sysfs` and `netlink` and thus does not require any external dependencies.
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `device` | Network interface to monitor (as specified in `/sys/class/net/`) | If not set, device will be automatically selected every `interval`
//! `format` | A string to customise the output of this block. See below for available placeholders. | `"$speed_down.eng(3,B,K)$speed_up.eng(3,B,K)"`
//! `format_alt` | If set, block will switch between `format` and `format_alt` on every click | `None`
//! `interval` | Update interval in seconds | `2`
//! `hide_missing` | Whether to hide interfaces that don't exist on the system. | `false`
//! `hide_inactive` | Whether to hide interfaces that are not connected (or missing). | `false`
//!
//! Placeholder       | Value                    | Type   | Unit
//! ------------------|--------------------------|--------|---------------
//! `speed_down`      | Download speed           | Number | Bytes per second
//! `speed_up`        | Upload speed             | Number | Bytes per second
//! `graph_down`      | Download speed graph     | Text   | -
//! `graph_up`        | Upload speed graph       | Text   | -
//! `device`          | The name of device       | Text   | -
//! `ssid`            | Netfork SSID (WiFi only) | Text   | -
//! `frequency`       | WiFi frequency           | Number | Hz
//! `signal_strength` | WiFi signal              | Number | %
//! `bitrate`         | WiFi connection bitrate  | Number | Bits per second
//!
//! # Example
//!
//! Display WiFi info if available
//!
//! ```toml
//! [[block]]
//! block = "net"
//! format = "{$signal_strength $ssid $frequency|Wired connection} via $device"
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
    device: Option<String>,
    format: FormatConfig,
    format_alt: Option<FormatConfig>,
    #[default(2.into())]
    interval: Seconds,
    hide_missing: bool,
    hide_inactive: bool,
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

    let mut widget = api.new_widget().with_format(format.clone());
    let mut timer = config.interval.timer();

    // Stats
    let mut stats = None;
    let mut stats_timer = Instant::now();
    let mut tx_hist = [0f64; 8];
    let mut rx_hist = [0f64; 8];

    loop {
        let mut speed_down: f64 = 0.0;
        let mut speed_up: f64 = 0.0;

        let device = NetDevice::from_interface(match &config.device {
            Some(i) => i.clone(),
            None => default_interface().await.unwrap_or_else(|| "lo".into()),
        })
        .await;

        match device {
            Some(device) if !device.is_up().await? => {
                if config.hide_inactive {
                    api.hide().await?;
                } else {
                    widget.set_text("×".to_string());
                    api.set_widget(&widget).await?;
                }
            }
            Some(device) => {
                widget.set_format(format.clone());

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

                wifi.ssid
                    .map(|s| values.insert("ssid".into(), Value::text(s)));
                wifi.frequency
                    .map(|f| values.insert("frequency".into(), Value::hertz(f)));
                wifi.signal
                    .map(|s| values.insert("signal_strength".into(), Value::percents(s)));
                wifi.bitrate
                    .map(|b| values.insert("bitrate".into(), Value::bits(b)));

                widget.set_values(values);
                widget.set_icon(device.icon)?;
                api.set_widget(&widget).await?;
            }
            None if config.hide_missing || config.hide_inactive => {
                api.hide().await?;
            }
            None => {
                return Err(Error::new("device not found"));
            }
        }

        loop {
            select! {
                _ = timer.tick() => break,
                event = api.event() => match event {
                    UpdateRequest => break,
                    Click(click) => {
                        if click.button == MouseButton::Left {
                            if let Some(format_alt) = &mut format_alt {
                                std::mem::swap(format_alt, &mut format);
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

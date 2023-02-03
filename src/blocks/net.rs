//! Network information
//!
//! This block uses `sysfs` and `netlink` and thus does not require any external dependencies.
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `device` | Network interface to monitor (as specified in `/sys/class/net/`). Supports regex. | If not set, device will be automatically selected every `interval`
//! `format` | A string to customise the output of this block. See below for available placeholders. | `" $icon ^icon_net_down $speed_down.eng(prefix:K) ^icon_net_up $speed_up.eng(prefix:K) "`
//! `format_alt` | If set, block will switch between `format` and `format_alt` on every click | `None`
//! `interval` | Update interval in seconds | `2`
//! `missing_format` | Same as `format` if the interface cannot be connected (or missing). | `" × "`
//!
//! Action          | Description                               | Default button
//! ----------------|-------------------------------------------|---------------
//! `toggle_format` | Toggles between `format` and `format_alt` | Left
//!
//! Placeholder       | Value                       | Type   | Unit
//! ------------------|-----------------------------|--------|---------------
//! `icon`            | Icon based on device's type | Icon   | -
//! `speed_down`      | Download speed              | Number | Bytes per second
//! `speed_up`        | Upload speed                | Number | Bytes per second
//! `graph_down`      | Download speed graph        | Text   | -
//! `graph_up`        | Upload speed graph          | Text   | -
//! `device`          | The name of device          | Text   | -
//! `ssid`            | Netfork SSID (WiFi only)    | Text   | -
//! `frequency`       | WiFi frequency              | Number | Hz
//! `signal_strength` | WiFi signal                 | Number | %
//! `bitrate`         | WiFi connection bitrate     | Number | Bits per second
//! `ip`              | IPv4 address of the iface   | Text   | -
//! `ipv6`            | IPv6 address of the iface   | Text   | -
//!
//! # Example
//!
//! Display WiFi info if available
//!
//! ```toml
//! [[block]]
//! block = "net"
//! format = " $icon {$signal_strength $ssid $frequency|Wired connection} via $device "
//! ```
//!
//! Display exact device
//!
//! ```toml
//! [[block]]
//! block = "net"
//! device = "^wlo0$"
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
use crate::netlink::NetDevice;
use crate::util;
use regex::Regex;
use std::time::Instant;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(default)]
pub struct Config {
    device: Option<String>,
    format: FormatConfig,
    format_alt: Option<FormatConfig>,
    missing_format: FormatConfig,
    #[default(2.into())]
    interval: Seconds,
}

pub async fn run(config: Config, mut api: CommonApi) -> Result<()> {
    api.set_default_actions(&[(MouseButton::Left, None, "toggle_format")])
        .await?;

    let mut format = config.format.with_default(
        " $icon ^icon_net_down $speed_down.eng(prefix:K) ^icon_net_up $speed_up.eng(prefix:K) ",
    )?;
    let missing_format = config.missing_format.with_default(" × ")?;
    let mut format_alt = match config.format_alt {
        Some(f) => Some(f.with_default("")?),
        None => None,
    };

    let mut widget = Widget::new().with_format(format.clone());
    let mut timer = config.interval.timer();

    let device_re = config
        .device
        .as_deref()
        .map(Regex::new)
        .transpose()
        .error("Failed to parse device regex")?;

    // Stats
    let mut stats = None;
    let mut stats_timer = Instant::now();
    let mut tx_hist = [0f64; 8];
    let mut rx_hist = [0f64; 8];

    loop {
        match NetDevice::new(device_re.as_ref()).await? {
            None => {
                widget.set_format(missing_format.clone());
                widget.set_values(default());
                api.set_widget(&widget).await?;
            }
            Some(device) if !device.is_up() => {
                widget.set_format(missing_format.clone());
                widget.set_values(default());
                api.set_widget(&widget).await?;
            }
            Some(device) => {
                widget.set_format(format.clone());

                let mut speed_down: f64 = 0.0;
                let mut speed_up: f64 = 0.0;

                // Calculate speed
                match (stats, device.iface.stats) {
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

                widget.set_values(map! {
                    "icon" => Value::icon(api.get_icon(device.icon)?),
                    "speed_down" => Value::bytes(speed_down),
                    "speed_up" => Value::bytes(speed_up),
                    "graph_down" => Value::text(util::format_bar_graph(&rx_hist)),
                    "graph_up" => Value::text(util::format_bar_graph(&tx_hist)),
                    [if let Some(v) = device.ip] "ip" => Value::text(v.to_string()),
                    [if let Some(v) = device.ipv6] "ipv6" => Value::text(v.to_string()),
                    [if let Some(v) = device.ssid()] "ssid" => Value::text(v),
                    [if let Some(v) = device.frequency()] "frequency" => Value::hertz(v),
                    [if let Some(v) = device.bitrate()] "bitrate" => Value::bits(v),
                    [if let Some(v) = device.signal()] "signal_strength" => Value::percents(v),
                    "device" => Value::text(device.iface.name),
                });

                api.set_widget(&widget).await?;
            }
        }

        loop {
            select! {
                _ = timer.tick() => break,
                event = api.event() => match event {
                    UpdateRequest => break,
                    Action(a) if a == "toggle_format" => {
                        if let Some(format_alt) = &mut format_alt {
                            std::mem::swap(format_alt, &mut format);
                            break;
                        }
                    }
                    _ => ()
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

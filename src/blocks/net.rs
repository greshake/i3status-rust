//! Network information
//!
//! This block uses `sysfs` and `netlink` and thus does not require any external dependencies.
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `device` | Network interface to monitor (as specified in `/sys/class/net/`). Supports regex. | If not set, device will be automatically selected every `interval`
//! `interval` | Update interval in seconds | `2`
//! `format` | A string to customise the output of this block. See below for available placeholders. | `" $icon ^icon_net_down $speed_down.eng(prefix:K) ^icon_net_up $speed_up.eng(prefix:K) "`
//! `format_alt` | If set, block will switch between `format` and `format_alt` on every click | `None`
//! `inactive_format` | Same as `format` but for when the interface is inactive | `" $icon Down "`
//! `missing_format` | Same as `format` but for when the device is missing | `" × "`
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
//! `nameserver`      | Nameserver                  | Text   | -
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
//! - `net_wireless` (as a progression)
//! - `net_up`
//! - `net_down`

use super::prelude::*;
use crate::netlink::NetDevice;
use crate::util;
use itertools::Itertools;
use regex::Regex;
use std::time::Instant;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub device: Option<String>,
    #[default(2.into())]
    pub interval: Seconds,
    pub format: FormatConfig,
    pub format_alt: Option<FormatConfig>,
    pub inactive_format: FormatConfig,
    pub missing_format: FormatConfig,
}

pub async fn run(config: &Config, api: &CommonApi) -> Result<()> {
    let mut actions = api.get_actions()?;
    api.set_default_actions(&[(MouseButton::Left, None, "toggle_format")])?;

    let mut format = config.format.with_default(
        " $icon ^icon_net_down $speed_down.eng(prefix:K) ^icon_net_up $speed_up.eng(prefix:K) ",
    )?;
    let missing_format = config.missing_format.with_default(" × ")?;
    let inactive_format = config.inactive_format.with_default(" $icon Down ")?;
    let mut format_alt = match &config.format_alt {
        Some(f) => Some(f.with_default("")?),
        None => None,
    };

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
                api.set_widget(Widget::new().with_format(missing_format.clone()))?;
            }
            Some(device) => {
                let mut widget = Widget::new();

                if device.is_up() {
                    widget.set_format(format.clone());
                } else {
                    widget.set_format(inactive_format.clone());
                }

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

                let icon = if let Some(signal) = device.signal() {
                    Value::icon_progression(device.icon, signal / 100.0)
                } else {
                    Value::icon(device.icon)
                };

                widget.set_values(map! {
                    "icon" => icon,
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
                    [if !device.nameservers.is_empty()] "nameserver" => Value::text(
                                                                            device
                                                                                .nameservers
                                                                                .into_iter()
                                                                                .map(|s| s.to_string())
                                                                                .join(" "),
                                                                        ),
                    "device" => Value::text(device.iface.name),
                });

                api.set_widget(widget)?;
            }
        }

        loop {
            select! {
                _ = timer.tick() => break,
                _ = api.wait_for_update_request() => break,
                Some(action) = actions.recv() => match action.as_ref() {
                    "toggle_format" => {
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

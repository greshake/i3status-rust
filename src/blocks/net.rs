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
//! `format` | A [MultiFormat][MaybeMultiFormatConfig] string to customise the output of this block. See below for available placeholders. | `[" $icon ^icon_net_down $speed_down.eng(prefix:K) ^icon_net_up $speed_up.eng(prefix:K) "]`
//! `inactive_format` | Same as `format` but for when the interface is inactive | `" $icon Down "`
//! `missing_format` | Same as `format` but for when the device is missing | `" × "`
//!
//! Action          | Description                               | Default button
//! ----------------|-------------------------------------------|---------------
//! `toggle_format` **DEPRECATED** | Toggles between `format` and `format_alt` | -
//! `next_format`  | Switches to the next format in the list     | Left
//! `prev_format`  | Switches to the previous format in the list | Right
//!
//! Placeholder       | Value                       | Type   | Unit
//! ------------------|-----------------------------|--------|---------------
//! `icon`            | Icon based on device's type | Icon   | -
//! `speed_down`      | Download speed              | Number | Bytes per second
//! `speed_up`        | Upload speed                | Number | Bytes per second
//! `graph_down`      | Download speed graph        | Text   | -
//! `graph_up`        | Upload speed graph          | Text   | -
//! `device`          | The name of device          | Text   | -
//! `ssid`            | Network SSID (WiFi only)    | Text   | -
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
//! - `net_loopback` (`$icon`)
//! - `net_vpn` (`$icon`)
//! - `net_wired` (`$icon`)
//! - `net_wireless` (`$icon`, as a progression)
//! - `net_up` (`^icon_net_up`)
//! - `net_down` (`^icon_net_down`)

use super::prelude::*;
use crate::netlink::NetDevice;
use crate::util;
use itertools::Itertools as _;
use regex::Regex;
use std::time::Instant;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(default)]
pub struct Config {
    pub device: Option<String>,
    #[default(2.into())]
    pub interval: Seconds,
    #[serde(flatten)]
    pub formats: MaybeMultiFormatConfig,
    pub inactive_format: FormatConfig,
    pub missing_format: FormatConfig,
}

pub(crate) fn prepare(config: &Config) -> Result<Arc<BlockPlan>> {
    // Every state that renders a device sets these on every render; the
    // wifi-only values (ssid, signal_strength, ...) and the addresses are
    // conditional and stay undeclared.
    let device_output = |output: OutputPlan| {
        output
            .icon("icon", IconChoices::fixed(NetDevice::ALL_ICONS))
    };
    let mut outputs = vec![
        device_output(OutputPlan::new(
            "main",
            config.format.with_default(
                " $icon ^icon_net_down $speed_down.eng(prefix:K) ^icon_net_up $speed_up.eng(prefix:K) ",
            )?,
        )),
        device_output(OutputPlan::new(
            "inactive",
            config.inactive_format.with_default(" $icon Down ")?,
        )),
        // `missing` sets no values at all: nothing to declare.
        OutputPlan::new("missing", config.missing_format.with_default(" × ")?),
    ];
    if let Some(format_alt) = &config.format_alt {
        outputs.push(device_output(OutputPlan::new(
            "alt",
            format_alt.with_default("")?,
        )));
    }
    BlockPlan::new(outputs)
}

pub async fn run(config: &Config, api: &CommonApi, plan: &Arc<BlockPlan>) -> Result<()> {
    let mut actions = api.get_actions()?;
    api.set_default_actions(&[
        (MouseButton::Left, None, "next_format"),
        (MouseButton::Right, None, "prev_format"),
    ])?;

    let output_main = plan.output("main")?;
    let output_inactive = plan.output("inactive")?;
    let output_missing = plan.output("missing")?;
    let output_alt = match &config.format_alt {
        Some(_) => Some(plan.output("alt")?),
        None => None,
    };
    let mut alt_shown = false;

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
                api.set_widget(output_missing.new_widget())?;
            }
            Some(device) => {
                let output = if device.is_up() {
                    match (&output_alt, alt_shown) {
                        (Some(alt), true) => alt,
                        _ => &output_main,
                    }
                } else {
                    &output_inactive
                };
                let mut widget = output.new_widget();

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
                    output.icon_progression("icon", device.icon, signal / 100.0)?
                } else {
                    output.named_icon_value("icon", device.icon)?
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
                    "toggle_format" if output_alt.is_some() => {
                        alt_shown = !alt_shown;
                        break;
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
    use super::*;

    #[test]
    fn plan_declares_device_icon_set_on_every_rendering_state() {
        let plan = prepare(&Config::default()).unwrap();
        let ids: Vec<_> = plan.outputs().map(|o| o.id()).collect();
        assert_eq!(ids, ["main", "inactive", "missing"]);

        for id in ["main", "inactive"] {
            let output = plan.output(id).unwrap();
            let choices = output.output().choices_for("icon").unwrap();
            for icon in NetDevice::ALL_ICONS {
                assert!(choices.permits(icon), "{id} must permit {icon}");
            }
            assert!(!choices.permits("net_up"));
        }

        // `missing` renders a bare format and sets no values at all.
        let missing = plan.output("missing").unwrap();
        assert_eq!(missing.output().icon_placeholders().count(), 0);
    }


    #[test]
    fn alt_output_exists_only_when_configured() {
        let plan = prepare(&Config::default()).unwrap();
        assert!(plan.output("alt").is_err());

        let config = Config {
            format_alt: Some(" $icon $device ".parse().unwrap()),
            ..Config::default()
        };
        let plan = prepare(&config).unwrap();
        let alt = plan.output("alt").unwrap();
        assert!(alt.format().contains_key("device"));
        let choices = alt.output().choices_for("icon").unwrap();
        assert!(choices.permits("net_wireless"));
    }

    #[test]
    fn every_chooser_icon_is_declared() {
        // Exercise every branch of the runtime icon chooser and check the
        // result against the declared set, so the two cannot drift apart.
        let mut seen = Vec::new();
        for is_wireless in [false, true] {
            for tun_wg_ppp in [false, true] {
                for name in ["lo", "eth0", "tun0", "wlan0"] {
                    let icon = NetDevice::icon_for(is_wireless, tun_wg_ppp, name);
                    assert!(
                        NetDevice::ALL_ICONS.contains(&icon),
                        "chooser returned undeclared icon {icon}"
                    );
                    if !seen.contains(&icon) {
                        seen.push(icon);
                    }
                }
            }
        }
        // ...and every declared name is actually reachable.
        assert_eq!(seen.len(), NetDevice::ALL_ICONS.len());
    }

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

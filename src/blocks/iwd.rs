//! Wi-Fi signal strength via iwd
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `format` | A string to customise the output of this block. | `" $icon $signal_strength "`
//! `disconnected_format` | Format shown when not connected | `" $icon "`
//!
//! Placeholder       | Value                        | Type   | Unit
//! ------------------|------------------------------|--------|-----
//! `icon`            | Icon based on signal level   | Icon   | -
//! `signal_strength` | Signal strength              | Number | %
//! `ssid`            | Connected network SSID       | Text   | -
//!
//! # Icons Used
//! - `net_wireless_iwd` (as a progression)
//! - `net_wireless_disconnected`

use std::ops::RangeBounds;

use iwdrs::session::Session;
use iwdrs::station::signal_level_agent::SignalLevelAgent;
use iwdrs::station::Station;
use tokio::sync::watch;

use super::prelude::*;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub format: FormatConfig,
    pub disconnected_format: FormatConfig,
}

struct Agent {
    tx: watch::Sender<f64>,
}

impl SignalLevelAgent for Agent {
    fn changed(&self, _station: &Station, signal_level: impl RangeBounds<i16>) {
        use std::ops::Bound;

        let lower = match signal_level.start_bound() {
            Bound::Included(&v) | Bound::Excluded(&v) => v,
            Bound::Unbounded => -100,
        };
        let upper = match signal_level.end_bound() {
            Bound::Included(&v) | Bound::Excluded(&v) => v,
            Bound::Unbounded => 0,
        };

        let mid = ((lower as i32 + upper as i32) / 2) as i16;
        let percent = (mid.clamp(-100, 0) as f64 + 100.0).max(0.0);
        let _ = self.tx.send(percent);
    }
}

pub async fn run(config: &Config, api: &CommonApi) -> Result<()> {
    let format = config.format.with_default(" $icon $signal_strength ")?;
    let disconnected_format = config.disconnected_format.with_default(" $icon ")?;

    let session = Session::new()
        .await
        .error("Failed to connect to iwd")?;

    loop {
        let stations = session.stations().await.error("Failed to list iwd stations")?;

        let Some(station) = stations.into_iter().next() else {
            let mut widget = Widget::new()
                .with_format(disconnected_format.clone())
                .with_state(State::Critical);
            widget.set_values(map!(
                "icon" => Value::icon("net_wireless_disconnected")
            ));
            api.set_widget(widget)?;
            select! {
                _ = api.wait_for_update_request() => continue,
                _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => continue,
            }
        };

        let (tx, mut rx) = watch::channel(0f64);

        let _agent = station
            .register_signal_level_agent(vec![-50, -65, -80], Agent { tx })
            .await
            .error("Failed to register signal-level agent")?;

        let mut state_stream = station
            .state_stream()
            .await
            .error("Failed to subscribe to station state")?;

        loop {
            let is_connected =
                matches!(station.state().await.ok(), Some(iwdrs::station::State::Connected));

            if !is_connected {
                let mut widget = Widget::new()
                    .with_format(disconnected_format.clone())
                    .with_state(State::Critical);
                widget.set_values(map!(
                    "icon" => Value::icon("net_wireless_disconnected")
                ));
                api.set_widget(widget)?;
            } else {
                let percent = *rx.borrow();

                let (icon_name, icon_value, state) = (
                    "net_wireless_iwd",
                    percent / 100.0,
                    if percent > 66.0 {
                        State::Good
                    } else if percent > 33.0 {
                        State::Warning
                    } else {
                        State::Critical
                    },
                );

                let ssid: Option<String> = match station.connected_network().await {
                    Ok(Some(net)) => net.name().await.ok(),
                    _ => None,
                };

                let mut widget = Widget::new().with_format(format.clone());
                let mut values = map!(
                    "icon"            => Value::icon_progression(icon_name, icon_value),
                    "signal_strength" => Value::percents(percent)
                );
                if let Some(s) = ssid {
                    values.insert("ssid".into(), Value::text(s));
                }
                widget.set_values(values);
                widget.state = state;
                api.set_widget(widget)?;
            }

            select! {
                changed = rx.changed() => { if changed.is_err() { break; } }
                ev = state_stream.next() => { if ev.is_none() { break; } }
                _ = api.wait_for_update_request() => {}
            }
        }
    }
}

use std::time::Duration;

use crossbeam_channel::Sender;
use serde_derive::Deserialize;

use nl80211::Socket;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::SharedConfig;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::formatting::value::Value;
use crate::formatting::FormatTemplate;
use crate::protocol::i3bar_event::I3BarEvent;
use crate::protocol::i3bar_event::MouseButton;
use crate::scheduler::Task;
use crate::widgets::text::TextWidget;
use crate::widgets::I3BarWidget;

pub struct Wifi {
    id: usize,
    output: TextWidget,
    format: FormatTemplate,
    format_alt: FormatTemplate,
    text_unavailable: String,
    clickable: bool,
    update_interval: Duration,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct WifiConfig {
    /// Update interval in seconds
    #[serde(deserialize_with = "deserialize_duration")]
    pub interval: Duration,

    /// Format string for displaying wifi information
    pub format: String,

    /// Click on the block to switch `format` to `format_alt`
    pub format_alt: String,

    /// Text that is displayed when no wifi connection is found
    pub text_unavailable: String,

    /// Wether to switch `format` with `format_alt` on click
    pub clickable: bool,
}

impl Default for WifiConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(10),
            format: "{signal}".to_string(),
            format_alt: "{ssid}".to_string(),
            text_unavailable: "x".to_string(),
            clickable: true,
        }
    }
}

impl ConfigBlock for Wifi {
    type Config = WifiConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        shared_config: SharedConfig,
        _tx_update_request: Sender<Task>,
    ) -> Result<Self> {
        Ok(Self {
            id,
            output: TextWidget::new(id, 0, shared_config)
                .with_text("...")
                .with_icon("net_wireless")?,
            format: FormatTemplate::from_string(&block_config.format)?,
            format_alt: FormatTemplate::from_string(&block_config.format_alt)?,
            text_unavailable: block_config.text_unavailable,
            clickable: block_config.clickable,
            update_interval: block_config.interval,
        })
    }
}

impl Block for Wifi {
    fn update(&mut self) -> Result<Option<Update>> {
        let interfaces = Socket::connect()
            .block_error("wifi", "failed to connect to the socket")?
            .get_interfaces_info()
            .block_error("wifi", "failed to get interfaces' information")?;

        let mut text = None;

        for interface in interfaces {
            if let Ok(ap) = interface.get_station_info() {
                // SSID is `None` when not connected
                if let Some(ssid) = interface.ssid {
                    let signal = signal_percents(nl80211::parse_i8(
                        &ap.signal.block_error("wifi", "failed to get signal")?,
                    ));

                    let frequency = nl80211::parse_u32(
                        &interface
                            .frequency
                            .block_error("wifi", "failed to get frequency")?,
                    ) * 1_000_000;

                    let interface = nl80211::parse_string(
                        &interface
                            .name
                            .block_error("wifi", "failed to get interface")?,
                    );

                    let values = map! {
                        "ssid" => Value::from_string(nl80211::parse_string(&ssid)),
                        "signal" => Value::from_integer(signal).percents(),
                        "frequency" => Value::from_float(frequency as f64).hertz(),
                        "interface" => Value::from_string(interface.trim_matches(char::from(0)).to_string()),
                    };

                    text = Some(self.format.render(&values)?);
                    break;
                }
            }
        }

        self.output
            .set_text(text.unwrap_or_else(|| self.text_unavailable.clone()));
        Ok(Some(self.update_interval.into()))
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.output]
    }

    fn click(&mut self, event: &I3BarEvent) -> Result<()> {
        if event.button == MouseButton::Left && self.clickable {
            std::mem::swap(&mut self.format, &mut self.format_alt);
            self.update()?;
        }
        Ok(())
    }

    fn id(&self) -> usize {
        self.id
    }
}

fn signal_percents(raw: i8) -> i64 {
    let raw = raw as f64;

    let perfect = -20.;
    let worst = -85.;
    let d = perfect - worst;

    // https://github.com/torvalds/linux/blob/9ff9b0d392ea08090cd1780fb196f36dbb586529/drivers/net/wireless/intel/ipw2x00/ipw2200.c#L4322-L4334
    let percents = 100. - (perfect - raw) * (15. * d + 62. * (perfect - raw)) / (d * d);

    (percents as i64).clamp(0, 100)
}

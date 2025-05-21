use bytes::Bytes;
use futures::SinkExt as _;

use serde::de;
use tokio::net::TcpStream;
use tokio::time::Interval;
use tokio_util::codec::{Framed, LengthDelimitedCodec};

use super::{BatteryDevice, BatteryInfo, BatteryStatus, DeviceName};
use crate::blocks::prelude::*;

make_log_macro!(debug, "battery[apc_ups]");

#[derive(Debug, SmartDefault)]
enum Value {
    String(String),
    // The value is a percentage (0-100)
    Percent(f64),
    Watts(f64),
    Seconds(f64),
    #[default]
    None,
}

impl<'de> Deserialize<'de> for Value {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        for unit in ["Percent", "Watts", "Seconds", "Minutes", "Hours"] {
            if let Some(stripped) = s.strip_suffix(unit) {
                let value = stripped.trim().parse::<f64>().map_err(de::Error::custom)?;
                return Ok(match unit {
                    "Percent" => Value::Percent(value),
                    "Watts" => Value::Watts(value),
                    "Seconds" => Value::Seconds(value),
                    "Minutes" => Value::Seconds(value * 60.0),
                    "Hours" => Value::Seconds(value * 3600.0),
                    _ => unreachable!(),
                });
            }
        }
        Ok(Value::String(s))
    }
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "UPPERCASE", default)]
struct Properties {
    status: Value,
    bcharge: Value,
    nompower: Value,
    loadpct: Value,
    timeleft: Value,
}

pub(super) struct Device {
    addr: String,
    interval: Interval,
}

impl Device {
    pub(super) async fn new(dev_name: DeviceName, interval: Seconds) -> Result<Self> {
        let addr = dev_name.exact().unwrap_or("localhost:3551");
        Ok(Self {
            addr: addr.to_string(),
            interval: interval.timer(),
        })
    }

    async fn get_status(&mut self) -> Result<Properties> {
        let mut conn = Framed::new(
            TcpStream::connect(&self.addr)
                .await
                .error("Failed to connect to socket")?,
            LengthDelimitedCodec::builder()
                .length_field_type::<u16>()
                .new_codec(),
        );

        conn.send(Bytes::from_static(b"status"))
            .await
            .error("Could not send message to socket")?;
        conn.close().await.error("Could not close socket sink")?;

        let mut map = serde_json::Map::new();

        while let Some(frame) = conn.next().await {
            let frame = frame.error("Failed to read from socket")?;
            if frame.is_empty() {
                continue;
            }
            let line = std::str::from_utf8(&frame).error("Failed to convert to UTF-8")?;
            let Some((key, value)) = line.split_once(':') else {
                debug!("Invalid field format: {line:?}");
                continue;
            };
            map.insert(
                key.trim().to_uppercase(),
                serde_json::Value::String(value.trim().to_string()),
            );
        }

        serde_json::from_value(serde_json::Value::Object(map)).error("Failed to deserialize")
    }
}

#[async_trait]
impl BatteryDevice for Device {
    async fn get_info(&mut self) -> Result<Option<BatteryInfo>> {
        let status_data = self
            .get_status()
            .await
            .map_err(|e| {
                debug!("{e}");
                e
            })
            .unwrap_or_default();

        let Value::String(status_str) = status_data.status else {
            return Ok(None);
        };

        let status = match &*status_str {
            "ONBATT" => BatteryStatus::Discharging,
            "ONLINE" => BatteryStatus::Charging,
            _ => BatteryStatus::Unknown,
        };

        // Even if the connection is valid, in the first few seconds
        // after apcupsd starts BCHARGE may not be present
        let Value::Percent(capacity) = status_data.bcharge else {
            return Ok(None);
        };

        let power = match (status_data.nompower, status_data.loadpct) {
            (Value::Watts(nominal_power), Value::Percent(load_percent)) => {
                Some(nominal_power * load_percent / 100.0)
            }
            _ => None,
        };

        let time_remaining = match status_data.timeleft {
            Value::Seconds(time_left) => Some(time_left),
            _ => None,
        };

        Ok(Some(BatteryInfo {
            status,
            capacity,
            power,
            time_remaining,
        }))
    }

    async fn wait_for_change(&mut self) -> Result<()> {
        self.interval.tick().await;
        Ok(())
    }
}

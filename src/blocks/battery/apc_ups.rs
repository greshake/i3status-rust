use std::str::FromStr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::Interval;

use super::{BatteryDevice, BatteryInfo, BatteryStatus, DeviceName};
use crate::blocks::prelude::*;

#[derive(Debug)]
struct PropertyMap(HashMap<String, String>);

impl PropertyMap {
    fn new() -> Self {
        Self(HashMap::new())
    }

    fn insert(&mut self, k: String, v: String) -> Option<String> {
        self.0.insert(k, v)
    }

    fn get(&self, k: &str) -> Option<&String> {
        self.0.get(k)
    }

    fn get_property<T: FromStr + Send + Sync>(
        &self,
        property_name: &str,
        required_unit: &str,
    ) -> Result<T> {
        if let Some(stat) = self.get(property_name) {
            let (value, unit) = stat
                .split_once(' ')
                .error(format!("could not split {}", property_name))
                .unwrap();
            if unit == required_unit {
                value
                    .parse::<T>()
                    .map_err(|_| Error::new("Could not parse data"))
            } else {
                return Err(Error::new(format!(
                    "Expected unit for {} are {}, but got {}",
                    property_name, required_unit, unit
                )));
            }
        } else {
            return Err(Error::new(format!("{} not in apc ups data", property_name)));
        }
    }
}

pub(super) struct Device {
    stream: TcpStream,
    interval: Interval,
}

impl Device {
    pub(super) async fn new(dev_name: DeviceName, interval: Seconds) -> Result<Self> {
        Ok(Self {
            stream: TcpStream::connect(dev_name.exact().unwrap_or("localhost:3551"))
                .await
                .error("Failed to connect to socket")?,
            interval: interval.timer(),
        })
    }

    async fn get_status(&mut self) -> Result<PropertyMap> {
        self.write(b"status").await?;
        let response = self.read_response().await?;

        let mut property_map = PropertyMap::new();

        for line in response.lines() {
            let (key, value) = line.split_once(':').unwrap();
            property_map.insert(key.trim().to_string(), value.trim().to_string());
        }

        Ok(property_map)
    }

    async fn write(&mut self, msg: &[u8]) -> Result<()> {
        match u16::try_from(msg.len()) {
            Ok(msg_len) => {
                self.stream
                    .write_u16(msg_len)
                    .await
                    .error("Could not write message length to socket")?;
                self.stream
                    .write_all(msg)
                    .await
                    .error("Could not write message to socket")?;
                Ok(())
            }
            _ => Err(Error::new(
                "msg is too long, it must be less than 2^16 characters long",
            )),
        }
    }

    async fn read_response(&mut self) -> Result<String> {
        let mut buf = String::new();
        loop {
            let read_size = self
                .stream
                .read_u16()
                .await
                .error("Could not read response length from socket")?
                .into();
            if read_size == 0 {
                break;
            }
            let mut read_buf = vec![0_u8; read_size];
            self.stream
                .read_exact(&mut read_buf)
                .await
                .error("Could not read from socket")?;
            buf.extend(String::from_utf8(read_buf));
        }
        Ok(buf)
    }
}

#[async_trait]
impl BatteryDevice for Device {
    async fn get_info(&mut self) -> Result<Option<BatteryInfo>> {
        let status_data = self.get_status().await?;

        let capacity = status_data.get_property::<f64>("BCHARGE", "Percent")?;

        let status = if let Some(status) = status_data.get("STATUS") {
            if status == "COMMLOST" {
                return Ok(None);
            } else if status == "ONBATT" {
                if capacity == 0.0 {
                    BatteryStatus::Empty
                } else {
                    BatteryStatus::Discharging
                }
            } else if status == "ONLINE" {
                if capacity == 100.0 {
                    BatteryStatus::Full
                } else {
                    BatteryStatus::Charging
                }
            } else {
                BatteryStatus::Unknown
            }
        } else {
            return Ok(None);
        };

        let power = status_data
            .get_property::<f64>("NOMPOWER", "Watts")
            .ok()
            .and_then(|nominal_power| {
                status_data
                    .get_property::<f64>("LOADPCT", "Percent")
                    .ok()
                    .map(|load_percent| nominal_power * load_percent / 100.0)
            });

        let time_remaining = status_data
            .get_property::<f64>("TIMELEFT", "Minutes")
            .ok()
            .map(|e| e * 60_f64);

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

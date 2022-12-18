use std::str::FromStr;
use tokio::net::TcpStream;
use tokio::time::Interval;

use super::{BatteryDevice, BatteryInfo, BatteryStatus, DeviceName};
use crate::blocks::prelude::*;

#[derive(Debug, Default)]
struct PropertyMap(HashMap<String, String>);

make_log_macro!(debug, "battery[apc_ups]");

impl PropertyMap {
    fn insert(&mut self, k: String, v: String) -> Option<String> {
        self.0.insert(k, v)
    }

    fn get(&self, k: &str) -> Option<&str> {
        self.0.get(k).map(|v| v.as_str())
    }

    fn get_property<T: FromStr + Send + Sync>(
        &self,
        property_name: &str,
        required_unit: &str,
    ) -> Result<T> {
        let stat = self
            .get(property_name)
            .or_error(|| format!("{property_name} not in apc ups data"))?;
        let (value, unit) = stat
            .split_once(' ')
            .or_error(|| format!("could not split {property_name}"))?;
        if unit == required_unit {
            value
                .parse::<T>()
                .map_err(|_| Error::new("Could not parse data"))
        } else {
            Err(Error::new(format!(
                "Expected unit for {property_name} are {required_unit}, but got {unit}"
            )))
        }
    }
}

#[derive(Debug)]
struct ApcConnection(TcpStream);

impl ApcConnection {
    async fn connect(addr: &str) -> Result<Self> {
        Ok(Self(
            TcpStream::connect(addr)
                .await
                .error("Failed to connect to socket")?,
        ))
    }

    async fn write(&mut self, msg: &[u8]) -> Result<()> {
        let msg_len = u16::try_from(msg.len())
            .error("msg is too long, it must be less than 2^16 characters long")?;

        self.0
            .write_u16(msg_len)
            .await
            .error("Could not write message length to socket")?;
        self.0
            .write_all(msg)
            .await
            .error("Could not write message to socket")?;
        Ok(())
    }

    async fn read_line<'a>(&'_ mut self, buf: &'a mut Vec<u8>) -> Result<Option<&'a str>> {
        let read_size = self
            .0
            .read_u16()
            .await
            .error("Could not read response length from socket")?
            .into();
        if read_size == 0 {
            return Ok(None);
        }

        buf.resize(read_size, 0);
        self.0
            .read_exact(buf)
            .await
            .error("Could not read from socket")?;

        std::str::from_utf8(buf).error("invalid UTF8").map(Some)
    }
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

    async fn get_status(&mut self) -> Result<PropertyMap> {
        let mut conn = ApcConnection::connect(&self.addr).await?;

        conn.write(b"status").await?;

        let mut buf = vec![];
        let mut property_map = PropertyMap::default();

        while let Some(line) = conn.read_line(&mut buf).await? {
            if let Some((key, value)) = line.split_once(':') {
                property_map.insert(key.trim().to_string(), value.trim().to_string());
            }
        }

        Ok(property_map)
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

        let status_str = status_data.get("STATUS").unwrap_or("COMMLOST");

        // Even if the connection is valid, in the first few seconds
        // after apcupsd starts BCHARGE may not be present
        let capacity = status_data
            .get_property::<f64>("BCHARGE", "Percent")
            .unwrap_or(f64::MIN);

        if status_str == "COMMLOST" || capacity == f64::MIN {
            return Ok(None);
        }

        let status = if status_str == "ONBATT" {
            if capacity == 0.0 {
                BatteryStatus::Empty
            } else {
                BatteryStatus::Discharging
            }
        } else if status_str == "ONLINE" {
            if capacity == 100.0 {
                BatteryStatus::Full
            } else {
                BatteryStatus::Charging
            }
        } else {
            BatteryStatus::Unknown
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

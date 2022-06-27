use super::{BatteryDevice, BatteryInfo, BatteryStatus, DeviceName};
use crate::blocks::prelude::*;
use std::io;
use std::str::FromStr;
use tokio::net::TcpStream;
use tokio::time::Interval;

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

    async fn get_status(&self) -> Result<HashMap<String, String>> {
        self.write(b"status").await?;

        let result = self.read_response().await?;

        let mut status_data: HashMap<String, String> = HashMap::new();
        for line in result.lines() {
            let (key, value) = line.split_once(':').unwrap();
            status_data.insert(String::from(key.trim()), String::from(value.trim()));
        }

        Ok(status_data)
    }

    async fn write(&self, msg: &[u8]) -> Result<()> {
        let msg_len = msg.len();
        if msg_len >= 1 << 16 {
            return Err(Error::new(
                "msg is too long, it must be less than 2^16 characters long",
            ));
        }

        // Write how many bytes of data are going to be sent (only need the last two octets)
        // Write the actual message
        let msgs = [&msg_len.to_be_bytes()[6..], msg];

        for msg in msgs {
            loop {
                // Wait for the socket to be writable
                self.stream
                    .writable()
                    .await
                    .error("Socket closed unexpectedly")?;

                // Try to write data, this may still fail with `WouldBlock`
                // if the readiness event is a false positive.
                match self.stream.try_write(msg) {
                    Ok(_) => {
                        break;
                    }
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                        continue;
                    }
                    Err(e) => {
                        return Err(Error::new(e.to_string()));
                    }
                }
            }
        }

        Ok(())
    }

    async fn read_exact(&self, n: usize) -> Result<Vec<u8>> {
        loop {
            // Wait for the socket to be readable
            self.stream
                .readable()
                .await
                .error("Socket closed unexpectedly")?;

            let mut buf = vec![0_u8; n];

            // Try to read data, this may still fail with `WouldBlock`
            // if the readiness event is a false positive.
            match self.stream.try_read(&mut buf) {
                Ok(_) => return Ok(buf),
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    continue;
                }
                Err(e) => {
                    return Err(Error::new(e.to_string()));
                }
            }
        }
    }

    async fn read_response(&self) -> Result<String> {
        let mut buf = String::new();
        loop {
            let read_info = self.read_exact(2).await?;
            let read_size = ((read_info[0] as usize) << 8) + (read_info[1] as usize);
            if read_size == 0 {
                break;
            }
            let read_buf = self.read_exact(read_size).await?;
            buf.extend(String::from_utf8(read_buf));
        }
        Ok(buf)
    }

    fn read_prop<T: FromStr + Send + Sync>(
        &self,
        status_data: &HashMap<String, String>,
        stat_name: &str,
        required_unit: &str,
    ) -> Result<T> {
        if let Some(stat) = status_data.get(stat_name) {
            let (value, unit) = stat
                .split_once(' ')
                .error(format!("could not split {}", stat_name))
                .unwrap();
            if unit == required_unit {
                value
                    .parse::<T>()
                    .map_err(|_| Error::new("Could not parse data"))
            } else {
                return Err(Error::new(format!(
                    "Expected unit for {} are {}, but got {}",
                    stat_name, required_unit, unit
                )));
            }
        } else {
            return Err(Error::new(format!("{} not in apc ups data", stat_name)));
        }
    }
}

#[async_trait]
impl BatteryDevice for Device {
    async fn get_info(&mut self) -> Result<Option<BatteryInfo>> {
        let status_data = self.get_status().await?;

        let capacity = self.read_prop::<f64>(&status_data, "BCHARGE", "Percent")?;

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

        let power = self
            .read_prop::<f64>(&status_data, "NOMPOWER", "Watts")
            .ok()
            .and_then(|nominal_power| {
                self.read_prop::<f64>(&status_data, "LOADPCT", "Percent")
                    .ok()
                    .map(|load_percent| nominal_power * load_percent / 100.0)
            });

        let time_remaining = self
            .read_prop::<f64>(&status_data, "TIMELEFT", "Minutes")
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

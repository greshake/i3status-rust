//! Display the stats of your AMD GPU
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `device` | The device in `/sys/class/drm/` to read from. | Any AMD card
//! `format` | A string to customise the output of this block. See below for available placeholders. | `" $icon $utilization "`
//! `format_alt` | If set, block will switch between `format` and `format_alt` on every click | `None`
//! `interval` | Update interval in seconds | `5`
//!
//! Placeholder          | Value                               | Type   | Unit
//! ---------------------|-------------------------------------|--------|------------
//! `icon`               | A static icon                       | Icon   | -
//! `utilization`        | GPU utilization                     | Number | %
//! `vram_total`         | Total VRAM                          | Number | Bytes
//! `vram_used`          | Used VRAM                           | Number | Bytes
//! `vram_used_percents` | Used VRAM / Total VRAM              | Number | %
//!
//! Action          | Description                               | Default button
//! ----------------|-------------------------------------------|---------------
//! `toggle_format` | Toggles between `format` and `format_alt` | Left
//!
//! # Example
//!
//! ```toml
//! [[block]]
//! block = "amd_gpu"
//! format = " $icon $utilization "
//! format_alt = " $icon MEM: $vram_used_percents ($vram_used/$vram_total) "
//! interval = 1
//! ```
//!
//! # Icons Used
//! - `gpu`

use std::path::PathBuf;
use std::str::FromStr;

use tokio::fs::read_dir;

use super::prelude::*;
use crate::util::read_file;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub device: Option<String>,
    pub format: FormatConfig,
    pub format_alt: Option<FormatConfig>,
    #[default(5.into())]
    pub interval: Seconds,
}

pub async fn run(config: &Config, api: &CommonApi) -> Result<()> {
    let mut actions = api.get_actions()?;
    api.set_default_actions(&[(MouseButton::Left, None, "toggle_format")])?;

    let mut format = config.format.with_default(" $icon $utilization ")?;
    let mut format_alt = match &config.format_alt {
        Some(f) => Some(f.with_default("")?),
        None => None,
    };

    let device = match &config.device {
        Some(name) => Device::new(name)?,
        None => Device::default_card()
            .await
            .error("failed to get default GPU")?
            .error("no GPU found")?,
    };

    loop {
        let mut widget = Widget::new().with_format(format.clone());

        let info = device.read_info().await?;

        widget.set_values(map! {
            "icon" => Value::icon("gpu"),
            "utilization" => Value::percents(info.utilization_percents),
            "vram_total" => Value::bytes(info.vram_total_bytes),
            "vram_used" => Value::bytes(info.vram_used_bytes),
            "vram_used_percents" => Value::percents(info.vram_used_bytes / info.vram_total_bytes * 100.0),
        });

        widget.state = match info.utilization_percents {
            x if x > 90.0 => State::Critical,
            x if x > 60.0 => State::Warning,
            x if x > 30.0 => State::Info,
            _ => State::Idle,
        };

        api.set_widget(widget)?;

        loop {
            select! {
                _ = sleep(config.interval.0) => break,
                _ = api.wait_for_update_request() => break,
                Some(action) = actions.recv() => match action.as_ref() {
                    "toggle_format" => {
                        if let Some(ref mut format_alt) = format_alt {
                            std::mem::swap(format_alt, &mut format);
                            break;
                        }
                    }
                    _ => (),
                }
            }
        }
    }
}

pub struct Device {
    path: PathBuf,
}

struct GpuInfo {
    utilization_percents: f64,
    vram_total_bytes: f64,
    vram_used_bytes: f64,
}

impl Device {
    fn new(name: &str) -> Result<Self, Error> {
        let path = PathBuf::from(format!("/sys/class/drm/{name}/device"));

        if !path.exists() {
            Err(Error::new(format!("Device {name} not found")))
        } else {
            Ok(Self { path })
        }
    }

    async fn default_card() -> std::io::Result<Option<Self>> {
        let mut dir = read_dir("/sys/class/drm").await?;

        while let Some(entry) = dir.next_entry().await? {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if !name.starts_with("card") {
                continue;
            }

            let mut path = entry.path();
            path.push("device");

            let Ok(uevent) = read_file(path.join("uevent")).await else {
                continue;
            };

            if uevent.contains("PCI_ID=1002") {
                return Ok(Some(Self { path }));
            }
        }

        Ok(None)
    }

    async fn read_prop<T: FromStr>(&self, prop: &str) -> Option<T> {
        read_file(self.path.join(prop))
            .await
            .ok()
            .and_then(|x| x.parse().ok())
    }

    async fn read_info(&self) -> Result<GpuInfo> {
        Ok(GpuInfo {
            utilization_percents: self
                .read_prop::<f64>("gpu_busy_percent")
                .await
                .error("Failed to read gpu_busy_percent")?,
            vram_total_bytes: self
                .read_prop::<f64>("mem_info_vram_total")
                .await
                .error("Failed to read mem_info_vram_total")?,
            vram_used_bytes: self
                .read_prop::<f64>("mem_info_vram_used")
                .await
                .error("Failed to read mem_info_vram_used")?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_non_existing_gpu_device() {
        let device = Device::new("/nope");
        assert!(device.is_err());
    }
}

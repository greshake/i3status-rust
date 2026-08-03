//! Display the stats of your AMD GPU
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `device` | The device in `/sys/class/drm/` to read from. | Any AMD card
//! `format` | A [MultiFormat][MaybeMultiFormatConfig] string to customise the output of this block. See below for available placeholders. | `[" $icon $utilization "]`
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
//! `toggle_format` **DEPRECATED** | Toggles between `format` and `format_alt` | -
//! `next_format`  | Switches to the next format in the list     | Left
//! `prev_format`  | Switches to the previous format in the list | Right
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
#[serde(default)]
pub struct Config {
    pub device: Option<String>,
    #[serde(flatten)]
    pub formats: MaybeMultiFormatConfig,
    #[default(5.into())]
    pub interval: Seconds,
}

pub(crate) fn prepare(config: &Config) -> Result<Arc<BlockPlan>> {
    // Every output renders the same value set, built unconditionally on
    // every update.
    let declare = |output: OutputPlan| output.icon("icon", IconChoices::one("gpu"));
    let formats = config.formats.with_default(" $icon $utilization ")?;
    BlockPlan::new(format_outputs(&formats, declare))
}

pub async fn run(config: &Config, api: &CommonApi, plan: &Arc<BlockPlan>) -> Result<()> {
    let mut actions = api.get_actions()?;
    api.set_default_actions(&[
        (MouseButton::Left, None, "next_format"),
        (MouseButton::Right, None, "prev_format"),
    ])?;

    let mut formats = FormatRotation::new(plan)?;

    let device = match &config.device {
        Some(name) => Device::new(name).await?,
        None => Device::default_card()
            .await
            .error("failed to get default GPU")?
            .error("no GPU found")?,
    };

    loop {
        let output = formats.current();
        let mut widget = output.new_widget();

        let info = device.read_info().await?;

        widget.set_values(map! {
            "icon" => output.icon_value("icon")?,
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
                    "next_format" | "toggle_format" => {
                        formats.next();
                        break;
                    }
                    "prev_format" => {
                        formats.prev();
                        break;
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
    async fn new(name: &str) -> Result<Self, Error> {
        let path = PathBuf::from(format!("/sys/class/drm/{name}/device"));

        if !tokio::fs::try_exists(&path)
            .await
            .error("Unable to stat file")?
        {
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

            if let Ok(uevent) = read_file(path.join("uevent")).await
                && uevent.contains("PCI_ID=1002")
            {
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

    #[tokio::test]
    async fn test_non_existing_gpu_device() {
        let device = Device::new("/nope").await;
        assert!(device.is_err());
    }

    #[test]
    fn plan_declares_single_format_with_gpu_icon() {
        let plan = prepare(&Config::default()).unwrap();
        let format = plan.output("format").unwrap();
        assert_eq!(format.single_icon("icon").unwrap(), "gpu");
        assert!(format.format().contains_key("utilization"));
        assert!(plan.output("format2").is_err());
    }

    #[test]
    fn every_configured_format_is_declared() {
        let config: Config =
            toml::from_str(r#"format = [" $icon $utilization ", " $icon $vram_used_percents "]"#)
                .unwrap();
        let plan = prepare(&config).unwrap();
        let ids: Vec<_> = plan.outputs().map(|o| o.id()).collect();
        assert_eq!(ids, ["format", "format2"]);
        let second = plan.output("format2").unwrap();
        assert!(second.format().contains_key("vram_used_percents"));
        assert_eq!(second.single_icon("icon").unwrap(), "gpu");
    }
}

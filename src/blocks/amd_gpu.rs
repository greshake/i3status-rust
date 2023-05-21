//! Display the stats of your AMD GPU
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `device` | The device in `/sys/class/drm/` to read from. | `"card0"`
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

use std::str::FromStr;

use super::prelude::*;
use crate::util::read_file;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    #[default("card0".into())]
    pub device: String,
    pub format: FormatConfig,
    pub format_alt: Option<FormatConfig>,
    #[default(5.into())]
    pub interval: Seconds,
}

pub async fn run(config: Config, mut api: CommonApi) -> Result<()> {
    api.set_default_actions(&[(MouseButton::Left, None, "toggle_format")])
        .await?;

    let mut format = config.format.with_default(" $icon $utilization ")?;
    let mut format_alt = match config.format_alt {
        Some(f) => Some(f.with_default("")?),
        None => None,
    };

    loop {
        let mut widget = Widget::new().with_format(format.clone());

        let info = read_gpu_info(&config.device).await?;

        widget.set_values(map! {
            "icon" => Value::icon(api.get_icon("gpu")?),
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

        api.set_widget(widget).await?;

        loop {
            select! {
                _ = sleep(config.interval.0) => break,
                event = api.event() => match event {
                    UpdateRequest => break,
                    Action(a) if a == "toggle_format" => {
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

struct GpuInfo {
    utilization_percents: f64,
    vram_total_bytes: f64,
    vram_used_bytes: f64,
}

async fn read_prop<T: FromStr>(device: &str, prop: &str) -> Option<T> {
    read_file(format!("/sys/class/drm/{device}/device/{prop}"))
        .await
        .ok()
        .and_then(|x| x.parse().ok())
}

async fn read_gpu_info(device: &str) -> Result<GpuInfo> {
    Ok(GpuInfo {
        utilization_percents: read_prop::<f64>(device, "gpu_busy_percent")
            .await
            .error("Failed to read gpu_busy_percent")?,
        vram_total_bytes: read_prop::<f64>(device, "mem_info_vram_total")
            .await
            .error("Failed to read mem_info_vram_total")?,
        vram_used_bytes: read_prop::<f64>(device, "mem_info_vram_used")
            .await
            .error("Failed to read mem_info_vram_used")?,
    })
}

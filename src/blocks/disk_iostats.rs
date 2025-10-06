//! Disk I/O statistics
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `device` | Block device or partition name to monitor (as specified in `/dev/`) | If not set, device will be automatically selected every `interval`
//! `format` | A string to customise the output of this block. See below for available placeholders. | `" $icon $speed_read.eng(prefix:K) $speed_write.eng(prefix:K) "`
//! `interval` | Update interval in seconds | `2`
//! `missing_format` | Same as `format` but for when the device is missing | `" × "`
//!
//! Placeholder | Value | Type   | Unit
//! ------------|-------|--------|-------
//! `icon` | A static icon | Icon | -
//! `device` | The name of device | Text | -
//! `speed_read` | Read speed | Number | Bytes per second
//! `speed_write` | Write speed | Number | Bytes per second
//!
//! # Examples
//!
//! ```toml
//! [[block]]
//! block = "disk_iostats"
//! device = "sda"
//! format = " $icon $speed_write.eng(prefix:K) "
//! ```
//!
//! Use labeled Games partition via persistent device names from /dev/disk/by-*/
//!
//! ```toml
//! [[block]]
//! block = "disk_iostats"
//! device = "disk/by-partlabel/Games"
//! format = " $icon $speed_write.eng(prefix:K) "
//! ```
//!
//! # Icons Used
//!
//! - `disk_drive`

use super::prelude::*;
use crate::util::read_file;
use libc::c_ulong;
use std::ops;
use std::path::Path;
use std::time::Instant;
use tokio::fs::read_dir;
use tokio::fs::read_link;

/// Path for block devices
const BLOCK_DEVICES_PATH: &str = "/sys/class/block";

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub device: Option<String>,
    #[default(2.into())]
    pub interval: Seconds,
    pub format: FormatConfig,
    pub missing_format: FormatConfig,
}

pub async fn run(config: &Config, api: &CommonApi) -> Result<()> {
    let format = config
        .format
        .with_default(" $icon $speed_read.eng(prefix:K) $speed_write.eng(prefix:K) ")?;
    let missing_format = config.missing_format.with_default(" × ")?;

    let mut timer = config.interval.timer();
    let mut old_stats = None;
    let mut stats_timer = Instant::now();

    loop {
        let mut device = config.device.clone();
        if device.is_none() {
            device = find_device().await?;
        }
        match device {
            None => {
                api.set_widget(Widget::new().with_format(missing_format.clone()))?;
            }
            Some(mut device) => {
                let mut widget = Widget::new();

                widget.set_format(format.clone());

                if let Ok(link) = read_link(Path::new("/dev/").join(&device)).await
                    && let Some(name) = link.file_name()
                    && let Ok(target) = name.to_os_string().into_string()
                {
                    device = target;
                }

                let new_stats = read_stats(&device).await?;
                let sector_size = read_sector_size(&device).await?;

                let mut speed_read = 0.0;
                let mut speed_write = 0.0;
                if let Some(old_stats) = old_stats {
                    let diff = new_stats - old_stats;
                    let elapsed = stats_timer.elapsed().as_secs_f64();
                    stats_timer = Instant::now();
                    let size_read = diff.sectors_read as u64 * sector_size;
                    let size_written = diff.sectors_written as u64 * sector_size;
                    speed_read = size_read as f64 / elapsed;
                    speed_write = size_written as f64 / elapsed;
                };
                old_stats = Some(new_stats);

                widget.set_values(map! {
                    "icon" => Value::icon("disk_drive"),
                    "speed_read" => Value::bytes(speed_read),
                    "speed_write" => Value::bytes(speed_write),
                    "device" => Value::text(device),
                });

                api.set_widget(widget)?;
            }
        }

        select! {
            _ = timer.tick() => continue,
            _ = api.wait_for_update_request() => continue,
        }
    }
}

async fn find_device() -> Result<Option<String>> {
    let mut sysfs_dir = read_dir(BLOCK_DEVICES_PATH)
        .await
        .error("Failed to open /sys/class/block directory")?;
    while let Some(dir) = sysfs_dir
        .next_entry()
        .await
        .error("Failed to read /sys/class/block directory")?
    {
        let path = dir.path();
        if path.join("device").exists() {
            return Ok(Some(
                dir.file_name()
                    .into_string()
                    .map_err(|_| Error::new("Invalid device filename"))?,
            ));
        }
    }

    Ok(None)
}

#[derive(Debug, Default, Clone, Copy)]
struct Stats {
    sectors_read: c_ulong,
    sectors_written: c_ulong,
}

impl ops::Sub for Stats {
    type Output = Self;

    fn sub(mut self, rhs: Self) -> Self::Output {
        self.sectors_read = self.sectors_read.wrapping_sub(rhs.sectors_read);
        self.sectors_written = self.sectors_written.wrapping_sub(rhs.sectors_written);
        self
    }
}

async fn read_stats(device: &str) -> Result<Stats> {
    let raw = read_file(Path::new(BLOCK_DEVICES_PATH).join(device).join("stat"))
        .await
        .error("Failed to read stat file")?;
    let fields: Vec<&str> = raw.split_whitespace().collect();
    Ok(Stats {
        sectors_read: fields
            .get(2)
            .error("Missing sectors read field")?
            .parse()
            .error("Failed to parse sectors read")?,
        sectors_written: fields
            .get(6)
            .error("Missing sectors written field")?
            .parse()
            .error("Failed to parse sectors written")?,
    })
}

async fn read_sector_size(device: &str) -> Result<u64> {
    if Path::new(BLOCK_DEVICES_PATH)
        .join(device)
        .join("device")
        .exists()
    {
        let raw = read_file(
            Path::new(BLOCK_DEVICES_PATH)
                .join(device)
                .join("queue/hw_sector_size"),
        )
        .await
        .error("Failed to read HW sector size")?;
        raw.parse::<u64>().error("Failed to parse HW sector size")
    } else {
        let mut sysfs_dir = read_dir(BLOCK_DEVICES_PATH)
            .await
            .error("Failed to open /sys/class/block directory")?;
        while let Some(dir) = sysfs_dir
            .next_entry()
            .await
            .error("Failed to read /sys/class/block directory")?
        {
            let path = dir.path();
            if path.join(device).exists() {
                let raw = read_file(path.join("queue/hw_sector_size"))
                    .await
                    .error("Failed to read partition HW sector size")?;
                return raw
                    .parse::<u64>()
                    .error("Failed to parse partition HW sector size");
            }
        }
        Err(Error::new(
            "Failed to find device for partition HW sector size",
        ))
    }
}

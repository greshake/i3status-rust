//! Memory and swap usage
//!
//! This module keeps track of both Swap and Memory. By default, a click switches between them.
//!
//! # Configuration
//!
//! Key | Values | Required | Default
//! ----|--------|----------|--------
//! `format_mem` | A string to customise the output of this block when in "Memory" view. See below for available placeholders. | No | `"$mem_free.eng(3,B,M)/$mem_total.eng(3,B,M)($mem_total_used_percents.eng(2))"`
//! `format_swap` | A string to customise the output of this block when in "Swap" view. See below for available placeholders. | No | `"$swap_free.eng(3,B,M)/$swap_total.eng(3,B,M)($swap_used_percents.eng(2))"`
//! `display_type` | Default view displayed on startup: "`memory`" or "`swap`" | No | `"memory"`
//! `clickable` | Whether the view should switch between memory and swap on click | No | `true`
//! `interval` | Update interval in seconds | No | `5`
//! `warning_mem` | Percentage of memory usage, where state is set to warning | No | `80.0`
//! `warning_swap` | Percentage of swap usage, where state is set to warning | No | `80.0`
//! `critical_mem` | Percentage of memory usage, where state is set to critical | No | `95.0`
//! `critical_swap` | Percentage of swap usage, where state is set to critical | No | `95.0`
//!
//! Placeholder               | Value                                                                         | Type   | Unit
//! --------------------------|-------------------------------------------------------------------------------|--------|-------
//! `mem_total`               | Memory total                                                                  | Number | Bytes
//! `mem_free`                | Memory free                                                                   | Number | Bytes
//! `mem_free_percents`       | Memory free                                                                   | Number | Percents
//! `mem_total_used`          | Total memory used                                                             | Number | Bytes
//! `mem_total_used_percents` | Total memory used                                                             | Number | Percents
//! `mem_used`                | Memory used, excluding cached memory and buffers; similar to htop's green bar | Number | Bytes
//! `mem_used_percents`       | Memory used, excluding cached memory and buffers; similar to htop's green bar | Number | Percents
//! `mem_avail`               | Available memory, including cached memory and buffers                         | Number | Bytes
//! `mem_avail_percents`      | Available memory, including cached memory and buffers                         | Number | Percents
//! `swap_total`              | Swap total                                                                    | Number | Bytes
//! `swap_free`               | Swap free                                                                     | Number | Bytes
//! `swap_free_percents`      | Swap free                                                                     | Number | Percents
//! `swap_used`               | Swap used                                                                     | Number | Bytes
//! `swap_used_percents`      | Swap used                                                                     | Number | Percents
//! `buffers`                 | Buffers, similar to htop's blue bar                                           | Number | Bytes
//! `buffers_percent`         | Buffers, similar to htop's blue bar                                           | Number | Percents
//! `cached`                  | Cached memory, similar to htop's yellow bar                                   | Number | Bytes
//! `cached_percent`          | Cached memory, similar to htop's yellow bar                                   | Number | Percents
//!
//! # Example
//!
//! ```toml
//! [[block]]
//! block = "memory"
//! format_mem = "mem_used_percents.eng(1)"
//! clickable = false
//! interval = 30
//! warning_mem = 70
//! critical_mem = 90
//! ```
//!
//! # Icons Used
//! - `memory_mem`
//! - `memory_swap`

use std::path::Path;
use std::str::FromStr;
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, BufReader};

use regex::Regex;

use super::prelude::*;
use crate::util::read_file;

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields, default)]
struct MemoryConfig {
    format_mem: FormatConfig,
    format_swap: FormatConfig,
    display_type: Memtype,
    clickable: bool,
    interval: u64,
    warning_mem: f64,
    warning_swap: f64,
    critical_mem: f64,
    critical_swap: f64,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            format_mem: Default::default(),
            format_swap: Default::default(),
            display_type: Memtype::Memory,
            clickable: true,
            interval: 5,
            warning_mem: 80.,
            warning_swap: 80.,
            critical_mem: 95.,
            critical_swap: 95.,
        }
    }
}

pub async fn run(config: toml::Value, mut api: CommonApi) -> Result<()> {
    let mut events = api.get_events().await?;
    let config = MemoryConfig::deserialize(config).config_error()?;
    let interval = Duration::from_secs(config.interval);

    let format_mem = config.format_mem.with_default(
        "$mem_free.eng(3,B,M)/$mem_total.eng(3,B,M)($mem_total_used_percents.eng(2))",
    )?;
    let format_swap = config
        .format_swap
        .with_default("$swap_free.eng(3,B,M)/$swap_total.eng(3,B,M)($swap_used_percents.eng(2))")?;

    let clickable = config.clickable;
    let mut memtype = config.display_type;
    let mut format = match memtype {
        Memtype::Memory => {
            api.set_icon("memory_mem")?;
            &format_mem
        }
        Memtype::Swap => {
            api.set_icon("memory_swap")?;
            &format_swap
        }
    };
    api.set_format(format.clone());

    loop {
        let mem_state = Memstate::new().await?;
        let mem_total = mem_state.mem_total as f64 * 1024.;
        let mem_free = mem_state.mem_free as f64 * 1024.;
        let swap_total = mem_state.swap_total as f64 * 1024.;
        let swap_free = mem_state.swap_free as f64 * 1024.;
        let swap_used = swap_total - swap_free;
        let mem_total_used = mem_total - mem_free;
        let buffers = mem_state.buffers as f64 * 1024.;
        let cached = (mem_state.cached + mem_state.s_reclaimable - mem_state.shmem) as f64 * 1024.
            + mem_state.zfs_arc_cache as f64;
        let mem_used = mem_total_used - (buffers + cached);
        let mem_avail = mem_total - mem_used;

        let values = map!(
            "mem_total" => Value::bytes(mem_total),
            "mem_free" => Value::bytes(mem_free),
            "mem_free_percents" => Value::percents(mem_free / mem_total * 100.),
            "mem_total_used" => Value::bytes(mem_total_used),
            "mem_total_used_percents" => Value::percents(mem_total_used / mem_total * 100.),
            "mem_used" => Value::bytes(mem_used),
            "mem_used_percents" => Value::percents(mem_used / mem_total * 100.),
            "mem_avail" => Value::bytes(mem_avail),
            "mem_avail_percents" => Value::percents(mem_avail / mem_total * 100.),
            "swap_total" => Value::bytes(swap_total),
            "swap_free" => Value::bytes(swap_free),
            "swap_free_percents" => Value::percents(swap_free / swap_total * 100.),
            "swap_used" => Value::bytes(swap_used),
            "swap_used_percents" => Value::percents(swap_used / swap_total * 100.),
            "buffers" => Value::bytes(buffers),
            "buffers_percent" => Value::percents(buffers / mem_total * 100.),
            "cached" => Value::bytes(cached),
            "cached_percent" => Value::percents(cached / mem_total * 100.),
        );
        api.set_values(values);

        api.set_state(match memtype {
            Memtype::Memory => match mem_used / mem_total * 100. {
                x if x > config.critical_mem => State::Critical,
                x if x > config.warning_mem => State::Warning,
                _ => State::Idle,
            },
            Memtype::Swap => match swap_used / swap_total * 100. {
                x if x > config.critical_swap => State::Critical,
                x if x > config.warning_swap => State::Warning,
                _ => State::Idle,
            },
        });

        api.flush().await?;

        tokio::select! {
            _ = sleep(interval) =>(),
            event = events.recv() => {
                if let BlockEvent::Click(click) = event.unwrap() {
                    if click.button == MouseButton::Left && clickable {
                        match memtype {
                            Memtype::Swap => {
                                format = &format_mem;
                                memtype = Memtype::Memory;
                                api.set_icon("memory_mem")?;
                            }
                            Memtype::Memory => {
                                format = &format_swap;
                                memtype = Memtype::Swap;
                                api.set_icon("memory_swap")?;
                            }
                        }
                        api.set_format(format.clone());
                    }
                }
            }
        }
    }
}

#[derive(serde_derive::Deserialize, Clone, Copy, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Memtype {
    Swap,
    Memory,
}

#[derive(Clone, Copy, Debug)]
// Not following naming convention, because of naming in /proc/meminfo
struct Memstate {
    mem_total: u64,
    mem_free: u64,
    buffers: u64,
    cached: u64,
    s_reclaimable: u64,
    shmem: u64,
    swap_total: u64,
    swap_free: u64,
    zfs_arc_cache: u64,
}

impl Memstate {
    async fn new() -> Result<Self> {
        let mut file = BufReader::new(
            File::open("/proc/meminfo")
                .await
                .error("/proc/meminfo does not exist")?,
        );

        let mut mem_state = Memstate {
            mem_total: 0,
            mem_free: 0,
            buffers: 0,
            cached: 0,
            s_reclaimable: 0,
            shmem: 0,
            swap_total: 0,
            swap_free: 0,
            zfs_arc_cache: 0,
        };

        let mut line = StdString::new();
        while file
            .read_line(&mut line)
            .await
            .error("failed to read /proc/meminfo")?
            != 0
        {
            let mut words = line.trim().split_whitespace();

            let name = match words.next() {
                Some(name) => name,
                None => {
                    line.clear();
                    continue;
                }
            };
            let val = words
                .next()
                .and_then(|x| u64::from_str(x).ok())
                .error("failed to parse /proc/meminfo")?;

            match name {
                "MemTotal:" => mem_state.mem_total = val,
                "MemFree:" => mem_state.mem_free = val,
                "Buffers:" => mem_state.buffers = val,
                "Cached:" => mem_state.cached = val,
                "SReclaimable:" => mem_state.s_reclaimable = val,
                "Shmem:" => mem_state.shmem = val,
                "SwapTotal:" => mem_state.swap_total = val,
                "SwapFree:" => mem_state.swap_free = val,
                _ => (),
            }

            line.clear();
        }

        // Read ZFS arc cache size to add to total cache size
        if let Ok(arcstats) = read_file(Path::new("/proc/spl/kstat/zfs/arcstats")).await {
            let size_re = Regex::new(r"size\s+\d+\s+(\d+)").unwrap(); // Valid regex is safe to unwrap.
            let size = &size_re
                .captures(&arcstats)
                .error("failed to find zfs_arc_cache size")?[1];
            mem_state.zfs_arc_cache =
                u64::from_str(size).error("failed to parse zfs_arc_cache size")?;
        }

        Ok(mem_state)
    }
}

//! Memory and swap usage
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `format` | A string to customise the output of this block when in "Memory" view. See below for available placeholders. | `" $icon $mem_free.eng(3,B,M)/$mem_total.eng(3,B,M)($mem_total_used_percents.eng(2)) "`
//! `format_alt` | If set, block will switch between `format` and `format_alt` on every click | `None`
//! `interval` | Update interval in seconds | `5`
//! `warning_mem` | Percentage of memory usage, where state is set to warning | `80.0`
//! `warning_swap` | Percentage of swap usage, where state is set to warning | `80.0`
//! `critical_mem` | Percentage of memory usage, where state is set to critical | `95.0`
//! `critical_swap` | Percentage of swap usage, where state is set to critical | `95.0`
//!
//! Placeholder               | Value                                                                         | Type   | Unit
//! --------------------------|-------------------------------------------------------------------------------|--------|-------
//! `icon`                    | Memory icon                                                                   | Icon   | -
//! `icon_swap`               | Swap icon                                                                     | Icon   | -
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
//! format = " $icon $mem_used_percents.eng(1) "
//! format_alt = " $icon_swap $swap_free.eng(3,B,M)/$swap_total.eng(3,B,M)($swap_used_percents.eng(2)) "
//! interval = 30
//! warning_mem = 70
//! critical_mem = 90
//! ```
//!
//! # Icons Used
//! - `memory_mem`
//! - `memory_swap`

use std::str::FromStr;
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, BufReader};

use super::prelude::*;
use crate::util::read_file;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
struct MemoryConfig {
    format: FormatConfig,
    format_alt: Option<FormatConfig>,
    #[default(5.into())]
    interval: Seconds,
    #[default(80.0)]
    warning_mem: f64,
    #[default(80.0)]
    warning_swap: f64,
    #[default(95.0)]
    critical_mem: f64,
    #[default(95.0)]
    critical_swap: f64,
}

pub async fn run(config: toml::Value, mut api: CommonApi) -> Result<()> {
    let config = MemoryConfig::deserialize(config).config_error()?;
    let mut widget = Widget::new();

    let mut format = config.format.with_default(
        " $icon $mem_free.eng(3,B,M)/$mem_total.eng(3,B,M)($mem_total_used_percents.eng(2)) ",
    )?;
    let mut format_alt = match config.format_alt {
        Some(f) => Some(f.with_default("")?),
        None => None,
    };

    let mut timer = config.interval.timer();

    loop {
        let mem_state = Memstate::new().await?;

        let mem_total = mem_state.mem_total as f64 * 1024.;
        let mem_free = mem_state.mem_free as f64 * 1024.;

        let mem_total_used = mem_total - mem_free;
        let buffers = mem_state.buffers as f64 * 1024.;
        let cached = (mem_state.cached + mem_state.s_reclaimable - mem_state.shmem) as f64 * 1024.
            + mem_state.zfs_arc_cache as f64;
        let mem_used = mem_total_used - (buffers + cached);
        let mem_avail = mem_total - mem_used;

        let swap_total = mem_state.swap_total as f64 * 1024.;
        let swap_free = mem_state.swap_free as f64 * 1024.;
        let swap_cached = mem_state.swap_cached as f64 * 1024.;
        let swap_used = swap_total - swap_free - swap_cached;

        widget.set_format(format.clone());
        widget.set_values(map! {
            "icon" => Value::icon(api.get_icon("memory_mem")?),
            "icon_swap" => Value::icon(api.get_icon("memory_swap")?),
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
            "cached_percent" => Value::percents(cached / mem_total * 100.)
        });

        let mem_state = match mem_used / mem_total * 100. {
            x if x > config.critical_mem => State::Critical,
            x if x > config.warning_mem => State::Warning,
            _ => State::Idle,
        };

        let swap_state = match swap_used / swap_total * 100. {
            x if x > config.critical_swap => State::Critical,
            x if x > config.warning_swap => State::Warning,
            _ => State::Idle,
        };

        widget.state = if mem_state == State::Critical || swap_state == State::Critical {
            State::Critical
        } else if mem_state == State::Warning || swap_state == State::Warning {
            State::Warning
        } else {
            State::Idle
        };

        api.set_widget(&widget).await?;

        loop {
            select! {
                _ = timer.tick() => break,
                event = api.event() => match event {
                    UpdateRequest => break,
                    Click(click) => {
                        if click.button == MouseButton::Left {
                            if let Some(ref mut format_alt) = format_alt {
                                std::mem::swap(format_alt, &mut format);
                                widget.set_format(format.clone());
                                break;
                            }
                        }
                    }

                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
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

        let mut mem_state = Memstate::default();
        let mut line = String::new();

        while file
            .read_line(&mut line)
            .await
            .error("failed to read /proc/meminfo")?
            != 0
        {
            let mut words = line.split_whitespace();

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
                "SwapCached:" => mem_state.swap_cached = val,
                _ => (),
            }

            line.clear();
        }

        // Read ZFS arc cache size to add to total cache size
        if let Ok(arcstats) = read_file("/proc/spl/kstat/zfs/arcstats").await {
            let size_re = regex!(r"size\s+\d+\s+(\d+)");
            let size = &size_re
                .captures(&arcstats)
                .error("failed to find zfs_arc_cache size")?[1];
            mem_state.zfs_arc_cache = size.parse().error("failed to parse zfs_arc_cache size")?;
        }

        Ok(mem_state)
    }
}

//! Memory and swap usage
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `format` | A string to customise the output of this block when in "Memory" view. See below for available placeholders. | `" $icon $mem_avail.eng(prefix:M)/$mem_total.eng(prefix:M)($mem_total_used_percents.eng(w:2)) "`
//! `format_alt` | If set, block will switch between `format` and `format_alt` on every click | `None`
//! `interval` | Update interval in seconds | `5`
//! `warning_mem` | Percentage of memory usage, where state is set to warning | `80.0`
//! `warning_swap` | Percentage of swap usage, where state is set to warning | `80.0`
//! `critical_mem` | Percentage of memory usage, where state is set to critical | `95.0`
//! `critical_swap` | Percentage of swap usage, where state is set to critical | `95.0`
//!
//! Placeholder               | Value                                                                           | Type   | Unit
//! --------------------------|---------------------------------------------------------------------------------|--------|-------
//! `icon`                    | Memory icon                                                                     | Icon   | -
//! `icon_swap`               | Swap icon                                                                       | Icon   | -
//! `mem_total`               | Total physical ram available                                                    | Number | Bytes
//! `mem_free`                | Free memory not yet used by the kernel or userspace (in general you should use mem_avail) | Number | Bytes
//! `mem_free_percents`       | as above but as a percentage of total memory                                    | Number | Percents
//! `mem_avail`               | Kernel estimate of usable free memory which includes cached memory and buffers  | Number | Bytes
//! `mem_avail_percents`      | as above but as a percentage of total memory                                    | Number | Percents
//! `mem_total_used`          | mem_total - mem_free                                                            | Number | Bytes
//! `mem_total_used_percents` | as above but as a percentage of total memory                                    | Number | Percents
//! `mem_used`                | Memory used, excluding cached memory and buffers; same as htop's green bar      | Number | Bytes
//! `mem_used_percents`       | as above but as a percentage of total memory                                    | Number | Percents
//! `buffers`                 | Buffers, similar to htop's blue bar                                             | Number | Bytes
//! `buffers_percent`         | as above but as a percentage of total memory                                    | Number | Percents
//! `cached`                  | Cached memory (taking into account ZFS ARC cache), similar to htop's yellow bar | Number | Bytes
//! `cached_percent`          | as above but as a percentage of total memory                                    | Number | Percents
//! `swap_total`              | Swap total                                                                      | Number | Bytes
//! `swap_free`               | Swap free                                                                       | Number | Bytes
//! `swap_free_percents`      | as above but as a percentage of total memory                                    | Number | Percents
//! `swap_used`               | Swap used                                                                       | Number | Bytes
//! `swap_used_percents`      | as above but as a percentage of total memory                                    | Number | Percents
//!
//! Action          | Description                               | Default button
//! ----------------|-------------------------------------------|---------------
//! `toggle_format` | Toggles between `format` and `format_alt` | Left
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

use std::cmp::min;
use std::str::FromStr;
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, BufReader};

use super::prelude::*;
use crate::util::read_file;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(default)]
pub struct Config {
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

pub async fn run(config: Config, mut api: CommonApi) -> Result<()> {
    api.set_default_actions(&[(MouseButton::Left, None, "toggle_format")])
        .await?;

    let mut widget = Widget::new();

    let mut format = config.format.with_default(
        " $icon $mem_avail.eng(prefix:M)/$mem_total.eng(prefix:M)($mem_total_used_percents.eng(w:2)) ",
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

        // TODO: possibly remove this as it is confusing to have `mem_total_used` and `mem_used`
        // htop and such only display equivalent of `mem_used`
        let mem_total_used = mem_total - mem_free;

        // dev note: difference between avail and free:
        // https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/commit/?id=34e431b0ae398fc54ea69ff85ec700722c9da773
        // same logic as htop
        let mem_avail = if mem_state.mem_available != 0 {
            min(mem_state.mem_available, mem_state.mem_total)
        } else {
            mem_state.mem_free
        } as f64
            * 1024.;

        let pagecache = mem_state.pagecache as f64 * 1024.;
        let reclaimable = mem_state.s_reclaimable as f64 * 1024.;
        let shmem = mem_state.shmem as f64 * 1024.;

        // TODO: see https://github.com/htop-dev/htop/pull/1003
        let zfs_arc_cache = mem_state.zfs_arc_cache as f64;

        // See https://lore.kernel.org/lkml/1455827801-13082-1-git-send-email-hannes@cmpxchg.org/
        let cached = pagecache + reclaimable - shmem + zfs_arc_cache;

        let buffers = mem_state.buffers as f64 * 1024.;

        // same logic as htop
        let used_diff = mem_free + buffers + pagecache + reclaimable;
        let mem_used = if mem_total >= used_diff {
            mem_total - used_diff
        } else {
            mem_total - mem_free
        };

        // account for ZFS ARC cache
        let mem_used = mem_used - zfs_arc_cache;

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
                    Action(a) if a == "toggle_format" => {
                        if let Some(ref mut format_alt) = format_alt {
                            std::mem::swap(format_alt, &mut format);
                            widget.set_format(format.clone());
                            break;
                        }
                    }
                    _ => (),
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Memstate {
    mem_total: u64,
    mem_free: u64,
    mem_available: u64,
    buffers: u64,
    pagecache: u64,
    s_reclaimable: u64,
    shmem: u64,
    swap_total: u64,
    swap_free: u64,
    swap_cached: u64,
    zfs_arc_cache: u64,
}

impl Memstate {
    async fn new() -> Result<Self> {
        // Reference: https://www.kernel.org/doc/Documentation/filesystems/proc.txt

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
                "MemAvailable:" => mem_state.mem_available = val,
                "Buffers:" => mem_state.buffers = val,
                "Cached:" => mem_state.pagecache = val,
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

//! Memory and swap usage
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `format` | A string to customise the output of this block when in "Memory" view. See below for available placeholders. | `" $icon $mem_used.eng(prefix:Mi)/$mem_total.eng(prefix:Mi)($mem_used_percents.eng(w:2)) "`
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
//! `zram_compressed`         | Compressed zram memory usage                                                    | Number | Bytes
//! `zram_decompressed`       | Decompressed zram memory usage                                                  | Number | Bytes
//! 'zram_comp_ratio'         | Ratio of the decompressed/compressed zram memory                                | Number | -
//! `zswap_compressed`        | Compressed zswap memory usage (>=Linux 5.19)                                    | Number | Bytes
//! `zswap_decompressed`      | Decompressed zswap memory usage (>=Linux 5.19)                                  | Number | Bytes
//! `zswap_decompressed_percents` | as above but as a percentage of total zswap memory  (>=Linux 5.19)          | Number | Percents
//! 'zswap_comp_ratio'        | Ratio of the decompressed/compressed zswap memory (>=Linux 5.19)                | Number | -
//!
//! Action          | Description                               | Default button
//! ----------------|-------------------------------------------|---------------
//! `toggle_format` | Toggles between `format` and `format_alt` | Left
//!
//! # Examples
//!
//! ```toml
//! [[block]]
//! block = "memory"
//! format = " $icon $mem_used_percents.eng(w:1) "
//! format_alt = " $icon_swap $swap_free.eng(w:3,u:B,p:Mi)/$swap_total.eng(w:3,u:B,p:Mi)($swap_used_percents.eng(w:2)) "
//! interval = 30
//! warning_mem = 70
//! critical_mem = 90
//! ```
//!
//! Show swap and hide if it is zero:
//!
//! ```toml
//! [[block]]
//! block = "memory"
//! format = " $icon $swap_used.eng(range:1..) |"
//! ```
//!
//! # Icons Used
//! - `memory_mem`
//! - `memory_swap`

use std::cmp::min;
use std::str::FromStr;
use tokio::fs::{read_dir, File};
use tokio::io::{AsyncBufReadExt, BufReader};

use super::prelude::*;
use crate::util::read_file;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub format: FormatConfig,
    pub format_alt: Option<FormatConfig>,
    #[default(5.into())]
    pub interval: Seconds,
    #[default(80.0)]
    pub warning_mem: f64,
    #[default(80.0)]
    pub warning_swap: f64,
    #[default(95.0)]
    pub critical_mem: f64,
    #[default(95.0)]
    pub critical_swap: f64,
}

pub async fn run(config: &Config, api: &CommonApi) -> Result<()> {
    let mut actions = api.get_actions()?;
    api.set_default_actions(&[(MouseButton::Left, None, "toggle_format")])?;

    let mut format = config.format.with_default(
        " $icon $mem_used.eng(prefix:Mi)/$mem_total.eng(prefix:Mi)($mem_used_percents.eng(w:2)) ",
    )?;
    let mut format_alt = match &config.format_alt {
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

        // While zfs_arc_cache can be considered "available" memory,
        // it can only free a maximum of (zfs_arc_cache - zfs_arc_min) amount.
        // see https://github.com/htop-dev/htop/pull/1003
        let zfs_shrinkable_size = mem_state
            .zfs_arc_cache
            .saturating_sub(mem_state.zfs_arc_min) as f64;
        let mem_avail = mem_avail + zfs_shrinkable_size;

        let pagecache = mem_state.pagecache as f64 * 1024.;
        let reclaimable = mem_state.s_reclaimable as f64 * 1024.;
        let shmem = mem_state.shmem as f64 * 1024.;

        // See https://lore.kernel.org/lkml/1455827801-13082-1-git-send-email-hannes@cmpxchg.org/
        let cached = pagecache + reclaimable - shmem + zfs_shrinkable_size;

        let buffers = mem_state.buffers as f64 * 1024.;

        // same logic as htop
        let used_diff = mem_free + buffers + pagecache + reclaimable;
        let mem_used = if mem_total >= used_diff {
            mem_total - used_diff
        } else {
            mem_total - mem_free
        };

        // account for ZFS ARC cache
        let mem_used = mem_used - zfs_shrinkable_size;

        let swap_total = mem_state.swap_total as f64 * 1024.;
        let swap_free = mem_state.swap_free as f64 * 1024.;
        let swap_cached = mem_state.swap_cached as f64 * 1024.;
        let swap_used = swap_total - swap_free - swap_cached;

        // Zswap usage
        let zswap_compressed = mem_state.zswap_compressed as f64 * 1024.;
        let zswap_decompressed = mem_state.zswap_decompressed as f64 * 1024.;

        let zswap_comp_ratio = if zswap_compressed != 0.0 {
            zswap_decompressed / zswap_compressed
        } else {
            0.0
        };
        let zswap_decompressed_percents = if (swap_used + swap_cached) != 0.0 {
            zswap_decompressed / (swap_used + swap_cached) * 100.0
        } else {
            0.0
        };

        // Zram usage
        let zram_compressed = mem_state.zram_compressed as f64;
        let zram_decompressed = mem_state.zram_decompressed as f64;

        let zram_comp_ratio = if zram_compressed != 0.0 {
            zram_decompressed / zram_compressed
        } else {
            0.0
        };

        let mut widget = Widget::new().with_format(format.clone());
        widget.set_values(map! {
            "icon" => Value::icon("memory_mem"),
            "icon_swap" => Value::icon("memory_swap"),
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
            "zram_compressed" => Value::bytes(zram_compressed),
            "zram_decompressed" => Value::bytes(zram_decompressed),
            "zram_comp_ratio" => Value::number(zram_comp_ratio),
            "zswap_compressed" => Value::bytes(zswap_compressed),
            "zswap_decompressed" => Value::bytes(zswap_decompressed),
            "zswap_decompressed_percents" => Value::percents(zswap_decompressed_percents),
            "zswap_comp_ratio" => Value::number(zswap_comp_ratio),
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

        api.set_widget(widget)?;

        loop {
            select! {
                _ = timer.tick() => break,
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
    zram_compressed: u64,
    zram_decompressed: u64,
    zswap_compressed: u64,
    zswap_decompressed: u64,
    zfs_arc_cache: u64,
    zfs_arc_min: u64,
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
                "Zswap:" => mem_state.zswap_compressed = val,
                "Zswapped:" => mem_state.zswap_decompressed = val,
                _ => (),
            }

            line.clear();
        }

        // For ZRAM
        let mut entries = read_dir("/sys/block/")
            .await
            .error("Could not read /sys/block")?;
        while let Some(entry) = entries
            .next_entry()
            .await
            .error("Could not get next file /sys/block")?
        {
            let Ok(file_name) = entry.file_name().into_string() else {
                continue;
            };
            if !file_name.starts_with("zram") {
                continue;
            }

            let zram_file_path = entry.path().join("mm_stat");
            let Ok(file) = File::open(zram_file_path).await else {
                continue;
            };

            let mut buf = BufReader::new(file);
            let mut line = String::new();
            if buf.read_to_string(&mut line).await.is_err() {
                continue;
            }

            let mut values = line.split_whitespace().map(|s| s.parse::<u64>());
            let (Some(Ok(zram_swap_size)), Some(Ok(zram_comp_size))) =
                (values.next(), values.next())
            else {
                continue;
            };

            // zram initializes with small amount by default, return 0 then
            if zram_swap_size >= 65_536 {
                mem_state.zram_decompressed += zram_swap_size;
                mem_state.zram_compressed += zram_comp_size;
            }
        }

        // For ZFS
        if let Ok(arcstats) = read_file("/proc/spl/kstat/zfs/arcstats").await {
            let size_re = regex!(r"size\s+\d+\s+(\d+)");
            let size = &size_re
                .captures(&arcstats)
                .error("failed to find zfs_arc_cache size")?[1];
            mem_state.zfs_arc_cache = size.parse().error("failed to parse zfs_arc_cache size")?;
            let c_min_re = regex!(r"c_min\s+\d+\s+(\d+)");
            let c_min = &c_min_re
                .captures(&arcstats)
                .error("failed to find zfs_arc_min size")?[1];
            mem_state.zfs_arc_min = c_min.parse().error("failed to parse zfs_arc_min size")?;
        }

        Ok(mem_state)
    }
}

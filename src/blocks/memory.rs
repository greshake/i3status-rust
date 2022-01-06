use regex::Regex;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::str::FromStr;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::SharedConfig;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::formatting::value::Value;
use crate::formatting::FormatTemplate;
use crate::protocol::i3bar_event::{I3BarEvent, MouseButton};
use crate::scheduler::Task;
use crate::widgets::text::TextWidget;
use crate::widgets::{I3BarWidget, State};

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Memtype {
    Swap,
    Memory,
}

#[derive(Clone, Copy, Debug)]
// Not following naming convention, because of naming in /proc/meminfo
struct Memstate {
    mem_total: (u64, bool),
    mem_free: (u64, bool),
    buffers: (u64, bool),
    cached: (u64, bool),
    s_reclaimable: (u64, bool),
    shmem: (u64, bool),
    swap_total: (u64, bool),
    swap_free: (u64, bool),
    zfs_arc_cache: u64,
}

impl Memstate {
    fn mem_total(&self) -> u64 {
        self.mem_total.0
    }

    fn mem_free(&self) -> u64 {
        self.mem_free.0
    }

    fn buffers(&self) -> u64 {
        self.buffers.0
    }

    fn cached(&self) -> u64 {
        self.cached.0
    }

    fn s_reclaimable(&self) -> u64 {
        self.s_reclaimable.0
    }

    fn shmem(&self) -> u64 {
        self.shmem.0
    }

    fn swap_total(&self) -> u64 {
        self.swap_total.0
    }

    fn swap_free(&self) -> u64 {
        self.swap_free.0
    }

    fn zfs_arc_cache(&self) -> u64 {
        self.zfs_arc_cache
    }

    fn new() -> Self {
        Memstate {
            mem_total: (0, false),
            mem_free: (0, false),
            buffers: (0, false),
            cached: (0, false),
            s_reclaimable: (0, false),
            shmem: (0, false),
            swap_total: (0, false),
            swap_free: (0, false),
            zfs_arc_cache: 0,
        }
    }

    fn done(&self) -> bool {
        self.mem_total.1
            && self.mem_free.1
            && self.buffers.1
            && self.cached.1
            && self.s_reclaimable.1
            && self.shmem.1
            && self.swap_total.1
            && self.swap_free.1
    }
}

#[derive(Clone, Debug)]
pub struct Memory {
    id: usize,
    memtype: Memtype,
    output: (TextWidget, TextWidget),
    clickable: bool,
    format: (FormatTemplate, FormatTemplate),
    update_interval: Duration,
    tx_update_request: Sender<Task>,
    warning: (f64, f64),
    critical: (f64, f64),
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct MemoryConfig {
    /// Format string for Memory view. All format values are described below.
    pub format_mem: FormatTemplate,

    /// Format string for Swap view.
    pub format_swap: FormatTemplate,

    /// Default view displayed on startup. Options are <br/> memory, swap
    pub display_type: Memtype,

    /// Whether the format string should be prepended with Icons. Options are <br/> true, false
    /// (Deprecated)
    pub icons: bool,

    /// Whether the view should switch between memory and swap on click. Options are <br/> true, false
    pub clickable: bool,

    /// The delay in seconds between an update. If `clickable`, an update is triggered on click. Integer values only.
    #[serde(deserialize_with = "deserialize_duration")]
    pub interval: Duration,

    /// Percentage of memory usage, where state is set to warning
    pub warning_mem: f64,

    /// Percentage of swap usage, where state is set to warning
    pub warning_swap: f64,

    /// Percentage of memory usage, where state is set to critical
    pub critical_mem: f64,

    /// Percentage of swap usage, where state is set to critical
    pub critical_swap: f64,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            format_mem: FormatTemplate::default(),
            format_swap: FormatTemplate::default(),
            display_type: Memtype::Memory,
            icons: true,
            clickable: true,
            interval: Duration::from_secs(5),
            warning_mem: 80.,
            warning_swap: 80.,
            critical_mem: 95.,
            critical_swap: 95.,
        }
    }
}

impl Memory {
    fn format_insert_values(&mut self, mem_state: Memstate) -> Result<(String, Option<String>)> {
        let mem_total = mem_state.mem_total() as f64 * 1024.;
        let mem_free = mem_state.mem_free() as f64 * 1024.;
        let swap_total = mem_state.swap_total() as f64 * 1024.;
        let swap_free = mem_state.swap_free() as f64 * 1024.;
        let swap_used = swap_total - swap_free;
        let mem_total_used = mem_total - mem_free;
        let buffers = mem_state.buffers() as f64 * 1024.;
        let cached =
            // Why do we include shared memory to "cached"?
            (mem_state.cached() + mem_state.s_reclaimable() - mem_state.shmem()) as f64 * 1024.
            + mem_state.zfs_arc_cache() as f64;
        let mem_used = mem_total_used - (buffers + cached);
        let mem_avail = mem_total - mem_used;

        let values = map!(
            "mem_total" => Value::from_float(mem_total).bytes(),
            "mem_free" => Value::from_float(mem_free).bytes(),
            "mem_free_percents" => Value::from_float(mem_free / mem_total * 100.).percents(),
            "mem_total_used" => Value::from_float(mem_total_used).bytes(),
            "mem_total_used_percents" => Value::from_float(mem_total_used / mem_total * 100.).percents(),
            "mem_used" => Value::from_float(mem_used).bytes(),
            "mem_used_percents" => Value::from_float(mem_used / mem_total * 100.).percents(),
            "mem_avail" => Value::from_float(mem_avail).bytes(),
            "mem_avail_percents" => Value::from_float(mem_avail / mem_total * 100.).percents(),
            "swap_total" => Value::from_float(swap_total).bytes(),
            "swap_free" => Value::from_float(swap_free).bytes(),
            "swap_free_percents" => Value::from_float(swap_free / swap_total * 100.).percents(),
            "swap_used" => Value::from_float(swap_used).bytes(),
            "swap_used_percents" => Value::from_float(swap_used / swap_total * 100.).percents(),
            "buffers" => Value::from_float(buffers).bytes(),
            "buffers_percent" => Value::from_float(buffers / mem_total * 100.).percents(),
            "cached" => Value::from_float(cached).bytes(),
            "cached_percent" => Value::from_float(cached / mem_total * 100.).percents(),
        );

        match self.memtype {
            Memtype::Memory => self.output.0.set_state(match mem_used / mem_total * 100. {
                x if x > self.critical.0 => State::Critical,
                x if x > self.warning.0 => State::Warning,
                _ => State::Idle,
            }),
            Memtype::Swap => self
                .output
                .1
                .set_state(match swap_used / swap_total * 100. {
                    x if x > self.critical.1 => State::Critical,
                    x if x > self.warning.1 => State::Warning,
                    _ => State::Idle,
                }),
        };

        Ok(match self.memtype {
            Memtype::Memory => self.format.0.render(&values)?,
            Memtype::Swap => self.format.1.render(&values)?,
        })
    }

    pub fn switch(&mut self) {
        let old: Memtype = self.memtype.clone();
        self.memtype = match old {
            Memtype::Memory => Memtype::Swap,
            _ => Memtype::Memory,
        };
    }
}

impl ConfigBlock for Memory {
    type Config = MemoryConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        shared_config: SharedConfig,
        tx: Sender<Task>,
    ) -> Result<Self> {
        let widget = TextWidget::new(id, 0, shared_config);
        Ok(Memory {
            id,
            memtype: block_config.display_type,
            output: if block_config.icons {
                (
                    widget.clone().with_icon("memory_mem")?,
                    widget.with_icon("memory_swap")?,
                )
            } else {
                (widget.clone(), widget)
            },
            clickable: block_config.clickable,
            format: (
                block_config
                    .format_mem
                    .with_default("{mem_free;M}/{mem_total;M}({mem_total_used_percents})")?,
                block_config
                    .format_swap
                    .with_default("{swap_free;M}/{swap_total;M}({swap_used_percents})")?,
            ),
            update_interval: block_config.interval,
            tx_update_request: tx,
            warning: (block_config.warning_mem, block_config.warning_swap),
            critical: (block_config.critical_mem, block_config.critical_swap),
        })
    }
}

impl Block for Memory {
    fn id(&self) -> usize {
        self.id
    }

    fn update(&mut self) -> Result<Option<Update>> {
        let f =
            File::open("/proc/meminfo").block_error("memory", "/proc/meminfo does not exist")?;
        let f = BufReader::new(f);

        let mut mem_state = Memstate::new();

        for line in f.lines() {
            // stop reading if all values are already present
            if mem_state.done() {
                break;
            }

            let line = match line {
                Ok(s) => s,
                _ => continue,
            };
            let line = line.split_whitespace().collect::<Vec<&str>>();

            match line.get(0) {
                Some(&"MemTotal:") => {
                    mem_state.mem_total = (
                        u64::from_str(line[1])
                            .block_error("memory", "failed to parse mem_total")?,
                        true,
                    );
                    continue;
                }
                Some(&"MemFree:") => {
                    mem_state.mem_free = (
                        u64::from_str(line[1]).block_error("memory", "failed to parse mem_free")?,
                        true,
                    );
                    continue;
                }
                Some(&"Buffers:") => {
                    mem_state.buffers = (
                        u64::from_str(line[1]).block_error("memory", "failed to parse buffers")?,
                        true,
                    );
                    continue;
                }
                Some(&"Cached:") => {
                    mem_state.cached = (
                        u64::from_str(line[1]).block_error("memory", "failed to parse cached")?,
                        true,
                    );
                    continue;
                }
                Some(&"SReclaimable:") => {
                    mem_state.s_reclaimable = (
                        u64::from_str(line[1])
                            .block_error("memory", "failed to parse s_reclaimable")?,
                        true,
                    );
                    continue;
                }
                Some(&"Shmem:") => {
                    mem_state.shmem = (
                        u64::from_str(line[1]).block_error("memory", "failed to parse shmem")?,
                        true,
                    );
                    continue;
                }
                Some(&"SwapTotal:") => {
                    mem_state.swap_total = (
                        u64::from_str(line[1])
                            .block_error("memory", "failed to parse swap_total")?,
                        true,
                    );
                    continue;
                }
                Some(&"SwapFree:") => {
                    mem_state.swap_free = (
                        u64::from_str(line[1])
                            .block_error("memory", "failed to parse swap_free")?,
                        true,
                    );
                    continue;
                }
                _ => {
                    continue;
                }
            }
        }

        // Read ZFS arc cache size to add to total cache size
        let zfs_arcstats_file = std::fs::read_to_string("/proc/spl/kstat/zfs/arcstats");
        if let Ok(arcstats) = zfs_arcstats_file {
            let size_re = Regex::new(r"size\s+\d+\s+(\d+)").unwrap(); // Valid regex is safe to unwrap.
            let size = &size_re
                .captures(&arcstats)
                .block_error("memory", "failed to find zfs_arc_cache size")?[1];
            mem_state.zfs_arc_cache =
                u64::from_str(size).block_error("memory", "failed to parse zfs_arc_cache size")?;
        }

        // Now, create the string to be shown
        let output_text = self.format_insert_values(mem_state)?;

        match self.memtype {
            Memtype::Memory => self.output.0.set_texts(output_text),
            Memtype::Swap => self.output.1.set_texts(output_text),
        }

        Ok(Some(self.update_interval.into()))
    }

    fn click(&mut self, event: &I3BarEvent) -> Result<()> {
        if event.button == MouseButton::Left && self.clickable {
            self.switch();
            self.update()?;
            self.tx_update_request.send(Task {
                id: self.id,
                update_time: Instant::now(),
            })?;
        }

        Ok(())
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![match self.memtype {
            Memtype::Memory => &self.output.0,
            Memtype::Swap => &self.output.1,
        }]
    }
}

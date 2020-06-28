use crossbeam_channel::Sender;
use serde_derive::Deserialize;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::str::FromStr;
use std::time::{Duration, Instant};
use uuid::Uuid;

use crate::blocks::Update;
use crate::blocks::{Block, ConfigBlock};
use crate::config::Config;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::formatter::{Bytes, Format, FormatTemplate};
use crate::input::{I3BarEvent, MouseButton};
use crate::scheduler::Task;
use crate::util::format_percent_bar;
use crate::widget::{I3BarWidget, State};
use crate::widgets::button::ButtonWidget;

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
    id: String,
    memtype: Memtype,
    output: (ButtonWidget, ButtonWidget),
    clickable: bool,
    format: (FormatTemplate, FormatTemplate),
    update_interval: Duration,
    tx_update_request: Sender<Task>,
    warning: (f64, f64),
    critical: (f64, f64),
}

#[derive(Deserialize, Debug, Clone)]
pub struct MemoryConfig {
    /// Format string for Memory view. All format values are described below.
    #[serde(default = "MemoryConfig::default_format_mem")]
    pub format_mem: String,

    /// Format string for Swap view.
    #[serde(default = "MemoryConfig::default_format_swap")]
    pub format_swap: String,

    /// Default view displayed on startup. Options are <br/> memory, swap
    #[serde(default = "MemoryConfig::default_display_type")]
    pub display_type: Memtype,

    /// Whether the format string should be prepended with Icons. Options are <br/> true, false
    #[serde(default = "MemoryConfig::default_icons")]
    pub icons: bool,

    /// Whether the view should switch between memory and swap on click. Options are <br/> true, false
    #[serde(default = "MemoryConfig::default_clickable")]
    pub clickable: bool,

    /// The delay in seconds between an update. If `clickable`, an update is triggered on click. Integer values only.
    #[serde(
        default = "MemoryConfig::default_interval",
        deserialize_with = "deserialize_duration"
    )]
    pub interval: Duration,

    /// Percentage of memory usage, where state is set to warning
    #[serde(default = "MemoryConfig::default_warning_mem")]
    pub warning_mem: f64,

    /// Percentage of swap usage, where state is set to warning
    #[serde(default = "MemoryConfig::default_warning_swap")]
    pub warning_swap: f64,

    /// Percentage of memory usage, where state is set to critical
    #[serde(default = "MemoryConfig::default_critical_mem")]
    pub critical_mem: f64,

    /// Percentage of swap usage, where state is set to critical
    #[serde(default = "MemoryConfig::default_critical_swap")]
    pub critical_swap: f64,
}

impl MemoryConfig {
    fn default_format_mem() -> String {
        "{MFm}MB/{MTm}MB({MUp}%)".to_owned()
    }

    fn default_format_swap() -> String {
        "{SFm}MB/{STm}MB({SUp}%)".to_owned()
    }

    fn default_display_type() -> Memtype {
        Memtype::Memory
    }

    fn default_icons() -> bool {
        true
    }

    fn default_clickable() -> bool {
        true
    }

    fn default_interval() -> Duration {
        Duration::from_secs(5)
    }

    fn default_warning_mem() -> f64 {
        80.0
    }

    fn default_warning_swap() -> f64 {
        80.0
    }

    fn default_critical_mem() -> f64 {
        95.0
    }

    fn default_critical_swap() -> f64 {
        95.0
    }
}

impl Memory {
    fn format_insert_values(&mut self, mem_state: Memstate) -> Result<String> {
        let mem_total = mem_state.mem_total() as f64;
        let mem_free = mem_state.mem_free() as f64;
        let swap_total = mem_state.swap_total() as f64;
        let swap_free = mem_state.swap_free() as f64;
        let swap_used = (mem_state.swap_total() - mem_state.swap_free()) as f64;
        let mem_total_used = mem_total - mem_free;
        let buffers = mem_state.buffers() as f64;
        let cached = (mem_state.cached() + mem_state.s_reclaimable() - mem_state.shmem()) as f64;
        let mem_used = mem_total_used - (buffers + cached);
        let mem_avail = mem_total - mem_used;

        let mem_free_percent = 100. * mem_free / mem_total;
        let mem_total_used_percent = 100. * mem_total_used / mem_total;
        let mem_used_percent = 100. * mem_used / mem_total;
        let mem_avail_percent = 100. * mem_avail / mem_total;
        let swap_free_percent = 100. * swap_free / swap_total;
        let swap_used_percent = 100. * swap_used / swap_total;
        let buffers_percent = 100. * buffers / swap_total;
        let cached_percent = 100. * cached / mem_total;

        let values = format_params! {
            "mem_total" => Bytes(1024. * mem_total),
            "mem_free" => Bytes(1024. * mem_free),
            "swap_total" => Bytes(1024. * swap_total),
            "swap_free" => Bytes(1024. * swap_free),
            "swap_used" => Bytes(1024. * swap_used),
            "mem_total_used" => Bytes(1024. * mem_total_used),
            "buffers" => Bytes(1024. * buffers),
            "cached" => Bytes(1024. * cached),
            "mem_used" => Bytes(1024. * mem_used),
            "mem_avail" => Bytes(1024. * mem_avail),
            "mem_free_percent" => mem_free_percent,
            "mem_free_bar" => format_percent_bar(mem_free_percent),
            "mem_total_used_percent" => mem_total_used_percent,
            "mem_total_used_bar" => format_percent_bar(mem_total_used_percent),
            "mem_used_percent" => mem_used_percent,
            "mem_used_bar" => format_percent_bar(mem_used_percent),
            "mem_avail_percent" => mem_avail_percent,
            "mem_avail_bar" => format_percent_bar(mem_avail_percent),
            "swap_free_percent" => swap_free_percent,
            "swap_free_bar" => format_percent_bar(swap_free_percent),
            "swap_used_percent" => swap_used_percent,
            "swap_used_bar" => format_percent_bar(swap_used_percent),
            "buffers_percent" => buffers_percent,
            "buffers_bar" => format_percent_bar(buffers_percent),
            "cached_percent" => cached_percent,
            "cached_bar" => format_percent_bar(cached_percent)
        };

        match self.memtype {
            Memtype::Memory => self.output.0.set_state(match mem_used_percent {
                x if x > self.critical.0 => State::Critical,
                x if x > self.warning.0 => State::Warning,
                _ => State::Idle,
            }),
            Memtype::Swap => self.output.1.set_state(match swap_used_percent {
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

    fn new(block_config: Self::Config, config: Config, tx: Sender<Task>) -> Result<Self> {
        let icons: bool = block_config.icons;
        let format = (
            FormatTemplate::from_string(&block_config.format_mem, &config.icons)?,
            FormatTemplate::from_string(&block_config.format_swap, &config.icons)?,
        );
        let widget = ButtonWidget::new(config, "memory").with_text("");
        Ok(Memory {
            id: Uuid::new_v4().to_simple().to_string(),
            memtype: block_config.display_type,
            output: if icons {
                (
                    widget.clone().with_icon("memory_mem"),
                    widget.with_icon("memory_swap"),
                )
            } else {
                (widget.clone(), widget)
            },
            clickable: block_config.clickable,
            format,
            update_interval: block_config.interval,
            tx_update_request: tx,
            warning: (block_config.warning_mem, block_config.warning_swap),
            critical: (block_config.critical_mem, block_config.critical_swap),
        })
    }
}

impl Block for Memory {
    fn id(&self) -> &str {
        &self.id
    }

    fn update(&mut self) -> Result<Option<Update>> {
        let f =
            File::open("/proc/meminfo").block_error("memory", "/proc/meminfo does not exist")?;
        let f = BufReader::new(f);

        let mut mem_state = Memstate::new();

        for line in f.lines() {
            if_debug!({
                let mut f = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open("/tmp/i3log")
                    .block_error("memory", "can't open /tmp/i3log")?;
                writeln!(f, "Updated: {:?}", mem_state)
                    .block_error("memory", "failed to write to /tmp/i3log")?;
            });

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

        // Now, create the string to be shown
        let output_text = self.format_insert_values(mem_state)?;

        match self.memtype {
            Memtype::Memory => self.output.0.set_text(output_text),
            Memtype::Swap => self.output.1.set_text(output_text),
        }

        if_debug!({
            let mut f = OpenOptions::new()
                .create(true)
                .append(true)
                .open("/tmp/i3log")
                .block_error("memory", "failed to open /tmp/i3log")?;
            writeln!(f, "Updated: {:?}", self)
                .block_error("memory", "failed to write to /tmp/i3log")?;
        });
        Ok(Some(self.update_interval.into()))
    }

    fn click(&mut self, event: &I3BarEvent) -> Result<()> {
        if_debug!({
            let mut f = OpenOptions::new()
                .create(true)
                .append(true)
                .open("/tmp/i3log")
                .block_error("memory", "failed to open /tmp/i3log")?;
            writeln!(f, "Click received: {:?}", event)
                .block_error("memory", "failed to write to /tmp/i3log")?;
        });

        if let Some(ref s) = event.name {
            if self.clickable && event.button == MouseButton::Left && *s == "memory" {
                self.switch();
                self.update()?;
                self.tx_update_request.send(Task {
                    id: self.id.clone(),
                    update_time: Instant::now(),
                })?;
            }
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

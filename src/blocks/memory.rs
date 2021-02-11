use std::collections::BTreeMap;
use std::fmt;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::str::FromStr;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::Config;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::input::{I3BarEvent, MouseButton};
use crate::scheduler::Task;
use crate::util::*;
use crate::widget::{I3BarWidget, State};
use crate::widgets::button::ButtonWidget;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Memtype {
    Swap,
    Memory,
}

#[derive(Clone, Copy)]
enum Unit {
    MiB(u64),
    GiB(f32),
    KiB(u64),
}

impl fmt::Display for Unit {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Unit::MiB(n) => n.fmt(f),
            Unit::KiB(n) => n.fmt(f),
            Unit::GiB(n) => n.fmt(f),
        }
    }
}

impl Unit {
    fn n(&self) -> u64 {
        match self.kib() {
            Unit::KiB(n) => n,
            _ => 0,
        }
    }

    fn gib(&self) -> Unit {
        match *self {
            Unit::KiB(n) => Unit::GiB((n as f32) / 1024f32.powi(2)),
            Unit::MiB(n) => Unit::GiB((n as f32) / 1024f32),
            Unit::GiB(n) => Unit::GiB(n),
        }
    }

    fn mib(&self) -> Unit {
        match *self {
            Unit::KiB(n) => Unit::MiB(n / 1024),
            Unit::MiB(n) => Unit::MiB(n),
            Unit::GiB(n) => Unit::MiB((n * 1024f32) as u64),
        }
    }

    fn kib(&self) -> Unit {
        match *self {
            Unit::KiB(n) => Unit::KiB(n),
            Unit::MiB(n) => Unit::KiB(n * 1024),
            Unit::GiB(n) => Unit::KiB((n * 1024f32.powi(2)) as u64),
        }
    }

    fn percent(&self, reference: Unit) -> f32 {
        if reference.n() < 1 {
            100f32
        } else {
            (self.n() as f32) / (reference.n() as f32) * 100f32
        }
    }
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
    id: usize,
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

    #[serde(default = "MemoryConfig::default_color_overrides")]
    pub color_overrides: Option<BTreeMap<String, String>>,
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

    fn default_color_overrides() -> Option<BTreeMap<String, String>> {
        None
    }
}

impl Memory {
    fn format_insert_values(&mut self, mem_state: Memstate) -> Result<String> {
        let mem_total = Unit::KiB(mem_state.mem_total());
        let mem_free = Unit::KiB(mem_state.mem_free());
        let swap_total = Unit::KiB(mem_state.swap_total());
        let swap_free = Unit::KiB(mem_state.swap_free());
        let swap_used = Unit::KiB(mem_state.swap_total() - mem_state.swap_free());
        let mem_total_used = Unit::KiB(mem_total.n() - mem_free.n());
        let buffers = Unit::KiB(mem_state.buffers());
        let cached = Unit::KiB(mem_state.cached() + mem_state.s_reclaimable() - mem_state.shmem());
        let mem_used = Unit::KiB(mem_total_used.n() - (buffers.n() + cached.n()));
        let mem_avail = Unit::KiB(mem_total.n() - mem_used.n());

        let values = map!(
            "{MTg}" => format!("{:.1}", mem_total.gib()),
            "{MTm}" => format!("{}", mem_total.mib()),
            "{MFg}" => format!("{:.1}", mem_free.gib()),
            "{MFm}" => format!("{}", mem_free.mib()),
            "{MFp}" => format!("{:.2}", mem_free.percent(mem_total)),
            "{MFpi}" => format!("{:02}", mem_free.percent(mem_total) as i32),
            "{MFpb}" => format_percent_bar(mem_free.percent(mem_total)),
            "{MUg}" => format!("{:.1}", mem_total_used.gib()),
            "{MUm}" => format!("{}", mem_total_used.mib()),
            "{MUp}" => format!("{:.2}", mem_total_used.percent(mem_total)),
            "{MUpi}" => format!("{:02}", mem_total_used.percent(mem_total) as i32),
            "{MUpb}" => format_percent_bar(mem_total_used.percent(mem_total)),
            "{Mug}" => format!("{:.1}", mem_used.gib()),
            "{Mum}" => format!("{}", mem_used.mib()),
            "{Mup}" => format!("{:.2}", mem_used.percent(mem_total)),
            "{Mupi}" => format!("{:02}", mem_used.percent(mem_total) as i32),
            "{Mupb}" => format_percent_bar(mem_used.percent(mem_total)),
            "{MAg}" => format!("{:.1}", mem_avail.gib()),
            "{MAm}" => format!("{}", mem_avail.mib()),
            "{MAp}" => format!("{:.2}", mem_avail.percent(mem_total)),
            "{MApi}" => format!("{:02}", mem_avail.percent(mem_total) as i32),
            "{MApb}" => format_percent_bar(mem_avail.percent(mem_total)),
            "{STg}" => format!("{:.1}", swap_total.gib()),
            "{STm}" => format!("{}", swap_total.mib()),
            "{SFg}" => format!("{:.1}", swap_free.gib()),
            "{SFm}" => format!("{}", swap_free.mib()),
            "{SFp}" => format!("{:.2}", swap_free.percent(swap_total)),
            "{SFpi}" => format!("{:02}", swap_free.percent(swap_total) as i32),
            "{SFpb}" => format_percent_bar(swap_free.percent(swap_total)),
            "{SUg}" => format!("{:.1}", swap_used.gib()),
            "{SUm}" => format!("{}", swap_used.mib()),
            "{SUp}" => format!("{:.2}", swap_used.percent(swap_total)),
            "{SUpi}" => format!("{:02}", swap_used.percent(swap_total) as i32),
            "{SUpb}" => format_percent_bar(swap_used.percent(swap_total)),
            "{Bg}" => format!("{:.1}", buffers.gib()),
            "{Bm}" => format!("{}", buffers.mib()),
            "{Bp}" => format!("{:.2}", buffers.percent(mem_total)),
            "{Bpi}" => format!("{:02}", buffers.percent(mem_total) as i32),
            "{Bpb}" => format_percent_bar(buffers.percent(mem_total)),
            "{Cg}" => format!("{:.1}", cached.gib()),
            "{Cm}" => format!("{}", cached.mib()),
            "{Cp}" => format!("{:.2}", cached.percent(mem_total)),
            "{Cpi}" => format!("{:02}", cached.percent(mem_total) as i32),
            "{Cpb}" => format_percent_bar(cached.percent(mem_total)));

        match self.memtype {
            Memtype::Memory => self.output.0.set_state(match mem_used.percent(mem_total) {
                x if f64::from(x) > self.critical.0 => State::Critical,
                x if f64::from(x) > self.warning.0 => State::Warning,
                _ => State::Idle,
            }),
            Memtype::Swap => self
                .output
                .1
                .set_state(match swap_used.percent(swap_total) {
                    x if f64::from(x) > self.critical.1 => State::Critical,
                    x if f64::from(x) > self.warning.1 => State::Warning,
                    _ => State::Idle,
                }),
        };

        Ok(match self.memtype {
            Memtype::Memory => self.format.0.render_static_str(&values)?,
            Memtype::Swap => self.format.1.render_static_str(&values)?,
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
        config: Config,
        tx: Sender<Task>,
    ) -> Result<Self> {
        let icons: bool = block_config.icons;
        let widget = ButtonWidget::new(config, id).with_text("");
        Ok(Memory {
            id,
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
            format: (
                FormatTemplate::from_string(&block_config.format_mem)?,
                FormatTemplate::from_string(&block_config.format_swap)?,
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

        // Now, create the string to be shown
        let output_text = self.format_insert_values(mem_state)?;

        match self.memtype {
            Memtype::Memory => self.output.0.set_text(output_text),
            Memtype::Swap => self.output.1.set_text(output_text),
        }

        Ok(Some(self.update_interval.into()))
    }

    fn click(&mut self, event: &I3BarEvent) -> Result<()> {
        if event.matches_id(self.id) && self.clickable {
            if let MouseButton::Left = event.button {
                self.switch();
                self.update()?;
                self.tx_update_request.send(Task {
                    id: self.id,
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

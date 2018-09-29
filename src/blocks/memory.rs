//! ## Memory
//!
//! Creates a block displaying memory and swap usage.
//!
//! By default, the format of this module is "<Icon>: {MFm}MB/{MTm}MB({MUp}%)" (Swap values
//! accordingly). That behaviour can be changed within config.json.
//!
//! This module keeps track of both Swap and Memory. By default, a click switches between them.
//!
//!
//! **Example**
//! ```javascript
//! {"block": "memory",
//!     "format_mem": "{Mum}MB/{MTm}MB({Mup}%)", "format_swap": "{SUm}MB/{STm}MB({SUp}%)",
//!     "type": "memory", "icons": true, "clickable": true, "interval": 5,
//!     "warning_mem": 80, "warning_swap": 80, "critical_mem": 95, "critical_swap": 95
//! },
//! ```
//!
//! **Options**
//!
//! Key | Values | Required | Default
//! ----|--------|----------|--------
//! format_mem | Format string for Memory view. All format values are described below. | No | {MFm}MB/{MTm}MB({MUp}%)
//! format_swap | Format string for Swap view. | No | {SFm}MB/{STm}MB({SUp}%)
//! type | Default view displayed on startup. Options are <br/> memory, swap | No | memory
//! icons | Whether the format string should be prepended with Icons. Options are <br/> true, false | No | true
//! clickable | Whether the view should switch between memory and swap on click. Options are <br/> true, false | No | true
//! interval | The delay in seconds between an update. If `clickable`, an update is triggered on click. Integer values only. | No | 5
//! warning_mem | Percentage of memory usage, where state is set to warning | No | 80.0
//! warning_swap | Percentage of swap usage, where state is set to warning | No | 80.0
//! critical_mem | Percentage of memory usage, where state is set to critical | No | 95.0
//! critical_swap | Percentage of swap usage, where state is set to critical | No | 95.0
//!
//! ### Format string specification
//!
//!  Key   | Value
//! -------|-------
//! {MTg}  | Memory total (GiB)
//! {MTm}  | Memory total (MiB)
//! {MAg}  | Available emory, including cached memory and buffers (GiB)
//! {MAm}  | Available memory, including cached memory and buffers (MiB)
//! {MAp}  | Available memory, including cached memory and buffers (%)
//! {MApi} | Available memory, including cached memory and buffers (%) as integer
//! {MFg}  | Memory free (GiB)
//! {MFm}  | Memory free (MiB)
//! {MFp}  | Memory free (%)
//! {MFpi} | Memory free (%) as integer
//! {Mug}  | Memory used, excluding cached memory and buffers; similar to htop's green bar (GiB)
//! {Mum}  | Memory used, excluding cached memory and buffers; similar to htop's green bar (MiB)
//! {Mup}  | Memory used, excluding cached memory and buffers; similar to htop's green bar (%)
//! {MUg}  | Total memory used (GiB)
//! {MUm}  | Total memory used (MiB)
//! {MUp}  | Total memory used (%)
//! {MUpi} | Total memory used (%) a integer
//! {Cg}   | Cached memory, similar to htop's yellow bar (GiB)
//! {Cm}   | Cached memory, similar to htop's yellow bar (MiB)
//! {Cp}   | Cached memory, similar to htop's yellow bar (%)
//! {Bg}   | Buffers, similar to htop's blue bar (GiB)
//! {Bm}   | Buffers, similar to htop's blue bar (MiB)
//! {Bp}   | Buffers, similar to htop's blue bar (%)
//! {Bpi}  | Buffers, similar to htop's blue bar (%) as integer
//! {STg}  | Swap total (GiB)
//! {STm}  | Swap total (MiB)
//! {SFg}  | Swap free (GiB)
//! {SFm}  | Swap free (MiB)
//! {SFp}  | Swap free (%)
//! {SFpi} | Swap free (%) as integer
//! {SUg}  | Swap used (GiB)
//! {SUm}  | Swap used (MiB)
//! {SUp}  | Swap used (%)
//! {SUpi} | Swap used (%) as integer

//!
use std::time::{Duration, Instant};
use std::collections::HashMap;
use util::*;
use chan::Sender;
use std::fs::File;
use std::io::{BufRead, BufReader};
use block::{Block, ConfigBlock};
use input::{I3BarEvent, MouseButton};
use std::str::FromStr;
use uuid::Uuid;
use std::fmt;

use config::Config;
use de::deserialize_duration;
use errors::*;
use widgets::button::ButtonWidget;
use widget::{I3BarWidget, State};
use scheduler::Task;

use std::io::Write;
use std::fs::OpenOptions;

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
        self.mem_total.1 && self.mem_free.1 && self.buffers.1 && self.cached.1 && self.s_reclaimable.1 && self.shmem.1 && self.swap_total.1 && self.swap_free.1
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
    values: HashMap<String, String>,
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
    #[serde(default = "MemoryConfig::default_interval", deserialize_with = "deserialize_duration")]
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
        let mem_total = Unit::KiB(mem_state.mem_total());
        let mem_free = Unit::KiB(mem_state.mem_free());
        let swap_total = Unit::KiB(mem_state.swap_total());
        let swap_free = Unit::KiB(mem_state.swap_free());
        let swap_used = Unit::KiB(mem_state.swap_total() - mem_state.swap_free());
        let mem_total_used = Unit::KiB(mem_total.n() - mem_free.n());
        let buffers = Unit::KiB(mem_state.buffers());
        let cached = Unit::KiB(
            mem_state.cached() + mem_state.s_reclaimable() - mem_state.shmem(),
        );
        let mem_used = Unit::KiB(mem_total_used.n() - (buffers.n() + cached.n()));
        let mem_avail = Unit::KiB(mem_total.n() - mem_used.n());

        self.values
            .insert("{MTg}".to_string(), format!("{:.1}", mem_total.gib()));
        self.values
            .insert("{MTm}".to_string(), format!("{}", mem_total.mib()));
        self.values
            .insert("{MFg}".to_string(), format!("{:.1}", mem_free.gib()));
        self.values
            .insert("{MFm}".to_string(), format!("{}", mem_free.mib()));
        self.values.insert(
            "{MFp}".to_string(),
            format!("{:.2}", mem_free.percent(mem_total)),
        );
        self.values.insert(
            "{MFpi}".to_string(),
            format!("{:02}", mem_free.percent(mem_total) as i32),
        );
        self.values
            .insert("{MUg}".to_string(), format!("{:.1}", mem_total_used.gib()));
        self.values
            .insert("{MUm}".to_string(), format!("{}", mem_total_used.mib()));
        self.values.insert(
            "{MUp}".to_string(),
            format!("{:.2}", mem_total_used.percent(mem_total)),
        );
        self.values.insert(
            "{MUpi}".to_string(),
            format!("{:02}", mem_total_used.percent(mem_total) as i32),
        );
        self.values
            .insert("{Mug}".to_string(), format!("{:.1}", mem_used.gib()));
        self.values
            .insert("{Mum}".to_string(), format!("{}", mem_used.mib()));
        self.values.insert(
            "{Mup}".to_string(),
            format!("{:.2}", mem_used.percent(mem_total)),
        );
        self.values.insert(
            "{Mupi}".to_string(),
            format!("{:02}", mem_used.percent(mem_total) as i32),
        );
        self.values
            .insert("{MAg}".to_string(), format!("{:.1}", mem_avail.gib()));
        self.values
            .insert("{MAm}".to_string(), format!("{}", mem_avail.mib()));
        self.values.insert(
            "{MAp}".to_string(),
            format!("{:.2}", mem_avail.percent(mem_total)),
        );
        self.values.insert(
            "{MApi}".to_string(),
            format!("{:02}", mem_avail.percent(mem_total) as i32),
        );
        self.values
            .insert("{STg}".to_string(), format!("{:.1}", swap_total.gib()));
        self.values
            .insert("{STm}".to_string(), format!("{}", swap_total.mib()));
        self.values
            .insert("{SFg}".to_string(), format!("{:.1}", swap_free.gib()));
        self.values
            .insert("{SFm}".to_string(), format!("{}", swap_free.mib()));
        self.values.insert(
            "{SFp}".to_string(),
            format!("{:.2}", swap_free.percent(swap_total)),
        );
        self.values.insert(
            "{SFpi}".to_string(),
            format!("{:02}", swap_free.percent(swap_total) as i32),
        );
        self.values
            .insert("{SUg}".to_string(), format!("{:.1}", swap_used.gib()));
        self.values
            .insert("{SUm}".to_string(), format!("{}", swap_used.mib()));
        self.values.insert(
            "{SUp}".to_string(),
            format!("{:.2}", swap_used.percent(swap_total)),
        );
        self.values.insert(
            "{SUpi}".to_string(),
            format!("{:02}", swap_used.percent(swap_total) as i32),
        );
        self.values
            .insert("{Bg}".to_string(), format!("{:.1}", buffers.gib()));
        self.values
            .insert("{Bm}".to_string(), format!("{}", buffers.mib()));
        self.values.insert(
            "{Bp}".to_string(),
            format!("{:.2}", buffers.percent(mem_total)),
        );
        self.values.insert(
            "{Bpi}".to_string(),
            format!("{:02}", buffers.percent(mem_total) as i32),
        );
        self.values
            .insert("{Cg}".to_string(), format!("{:.1}", cached.gib()));
        self.values
            .insert("{Cm}".to_string(), format!("{}", cached.mib()));
        self.values.insert(
            "{Cp}".to_string(),
            format!("{:.2}", cached.percent(mem_total)),
        );
        self.values.insert(
            "{Cpi}".to_string(),
            format!("{:02}", cached.percent(mem_total) as i32),
        );

        match self.memtype {
            Memtype::Memory => self.output.0.set_state(match mem_used.percent(mem_total) {
                x if x as f64 > self.critical.0 => State::Critical,
                x if x as f64 > self.warning.0 => State::Warning,
                _ => State::Idle,
            }),
            Memtype::Swap => self.output.1.set_state(
                match swap_used.percent(swap_total) {
                    x if x as f64 > self.critical.1 => State::Critical,
                    x if x as f64 > self.warning.1 => State::Warning,
                    _ => State::Idle,
                },
            ),
        };

        if_debug!({
            let mut f = OpenOptions::new()
                .create(true)
                .append(true)
                .open("/tmp/i3log")
                .block_error("memory", "can't open /tmp/i3log")?;
            writeln!(f, "Inserted values: {:?}", self.values)
                .block_error("memory", "failed to write to /tmp/i3log")?;
        });

        Ok(match self.memtype {
            Memtype::Memory => self.format.0.render(&self.values),
            Memtype::Swap => self.format.1.render(&self.values),
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
        let widget = ButtonWidget::new(config, "memory").with_text("");
        Ok(Memory {
            id: format!("{}", Uuid::new_v4().to_simple()),
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
                FormatTemplate::from_string(block_config.format_mem)?,
                FormatTemplate::from_string(block_config.format_swap)?,
            ),
            update_interval: block_config.interval,
            tx_update_request: tx,
            values: HashMap::<String, String>::new(),
            warning: (block_config.warning_mem, block_config.warning_swap),
            critical: (block_config.critical_mem, block_config.critical_swap),
        })
    }
}


impl Block for Memory {
    fn id(&self) -> &str {
        &self.id
    }

    fn update(&mut self) -> Result<Option<Duration>> {
        let f = File::open("/proc/meminfo")
            .block_error("memory", "/proc/meminfo does not exist")?;
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

            match line[0] {
                "MemTotal:" => {
                    mem_state.mem_total = (
                        u64::from_str(line[1])
                            .block_error("memory", "failed to parse mem_total")?,
                        true,
                    );
                    continue;
                }
                "MemFree:" => {
                    mem_state.mem_free = (
                        u64::from_str(line[1])
                            .block_error("memory", "failed to parse mem_free")?,
                        true,
                    );
                    continue;
                }
                "Buffers:" => {
                    mem_state.buffers = (
                        u64::from_str(line[1])
                            .block_error("memory", "failed to parse buffers")?,
                        true,
                    );
                    continue;
                }
                "Cached:" => {
                    mem_state.cached = (
                        u64::from_str(line[1])
                            .block_error("memory", "failed to parse cached")?,
                        true,
                    );
                    continue;
                }
                "SReclaimable:" => {
                    mem_state.s_reclaimable = (
                        u64::from_str(line[1])
                            .block_error("memory", "failed to parse s_reclaimable")?,
                        true,
                    );
                    continue;
                }
                "Shmem:" => {
                    mem_state.shmem = (
                        u64::from_str(line[1])
                            .block_error("memory", "failed to parse shmem")?,
                        true,
                    );
                    continue;
                }
                "SwapTotal:" => {
                    mem_state.swap_total = (
                        u64::from_str(line[1])
                            .block_error("memory", "failed to parse swap_total")?,
                        true,
                    );
                    continue;
                }
                "SwapFree:" => {
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
        Ok(Some(self.update_interval))
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
                });
            }
        }

        Ok(())
    }

    fn view(&self) -> Vec<&I3BarWidget> {
        vec![
            match self.memtype {
                Memtype::Memory => &self.output.0,
                Memtype::Swap => &self.output.1,
            },
        ]
    }
}

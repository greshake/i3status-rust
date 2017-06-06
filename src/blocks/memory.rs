//! ## Memory
//!
//! Creates a block displaying memory and swap usage.
//!
//! By default, the format of this module is "<Icon>: {MFm}MB/{MTm}MB({Mp}%)" (Swap values
//! accordingly). That behaviour can be changed within config.json.
//!
//! This module keeps track of both Swap and Memory. By default, a click switches between them.
//!
//!
//! **Example**
//! ```javascript
//! {"block": "memory",
//!     "format_mem": "{Mum}MB/{MTm}MB({Mup}%)", "format_swap": "{SUm}MB/{STm}MB({SUp}%)",
//!     "type": "memory", "icons": "true", "clickable": "true", "interval": "5",
//!     "warning_mem": 80, "warning_swap": 80, "critical_mem": 95, "critical_swap": 95
//! },
//! ```
//!
//! **Options**
//!
//! Key | Values | Required | Default
//! ----|--------|----------|--------
//! format_mem | Format string for Memory view. All format values are described below. | No | {MFm}MB/{MTm}MB({Mp}%)
//! format_swap | Format string for Swap view. | No | {SFm}MB/{STm}MB({Sp}%)
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
//! Key | Value
//! ----|-------
//! {MTg} | Memory total (GiB)
//! {MTm} | Memory total (MiB)
//! {MAg} | Available emory, including cached memory and buffers (GiB)
//! {MAm} | Available memory, including cached memory and buffers (MiB)
//! {MAp} | Available memory, including cached memory and buffers (%)
//! {MFg} | Memory free (GiB)
//! {MFm} | Memory free (MiB)
//! {MFp} | Memory free (%)
//! {Mug} | Memory used, excluding cached memory and buffers; similar to htop's green bar (GiB)
//! {Mum} | Memory used, excluding cached memory and buffers; similar to htop's green bar (MiB)
//! {Mup} | Memory used, excluding cached memory and buffers; similar to htop's green bar (%)
//! {MUg} | Total memory used (GiB)
//! {MUm} | Total memory used (MiB)
//! {MUp} | Total memory used (%)
//! {Cg}  | Cached memory, similar to htop's yellow bar (GiB)
//! {Cm}  | Cached memory, similar to htop's yellow bar (MiB)
//! {Cp}  | Cached memory, similar to htop's yellow bar (%)
//! {Bg}  | Buffers, similar to htop's blue bar (GiB)
//! {Bm}  | Buffers, similar to htop's blue bar (MiB)
//! {Bp}  | Buffers, similar to htop's blue bar (%)
//! {STg} | Swap total (GiB)
//! {STm} | Swap total (MiB)
//! {SFg} | Swap free (GiB)
//! {SFm} | Swap free (MiB)
//! {SFp} | Swap free (%)
//! {SUg} | Swap used (GiB)
//! {SUm} | Swap used (MiB)
//! {SUp} | Swap used (%)
//!
//!

use std::time::{Instant, Duration};
use std::collections::HashMap;
use util::*;
use std::sync::mpsc::Sender;
use std::fs::File;
use std::io::{BufReader, BufRead};
use block::Block;
use input::I3barEvent;
use std::str::FromStr;
use serde_json::Value;
use uuid::Uuid;
use std::fmt;


use widgets::button::ButtonWidget;
use widget::{State,I3BarWidget};
use scheduler::Task;


use std::io::Write;
use std::fs::OpenOptions;


#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Memtype {
    SWAP,
    MEMORY
}

impl Memtype {
    fn num(&self) -> usize {
        match *self {
            Memtype::MEMORY => 0,
            Memtype::SWAP => 1
        }
    }
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
            _ => 0
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
        if reference.n()<1 {
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
    swap_free: (u64, bool)
}

impl Memstate {
    fn mem_total(&self) -> u64 { self.mem_total.0 }
    fn mem_free(&self) -> u64 { self.mem_free.0 }
    fn buffers(&self) -> u64 { self.buffers.0 }
    fn cached(&self) -> u64 { self.cached.0 }
    fn s_reclaimable(&self) -> u64 { self.s_reclaimable.0 }
    fn shmem(&self) -> u64 { self.shmem.0 }
    fn swap_total(&self) -> u64 { self.swap_total.0 }
    fn swap_free(&self) -> u64 { self.swap_free.0 }
    fn new() -> Self {
        Memstate {
            mem_total: (0, false),
            mem_free: (0, false),
            buffers: (0, false),
            cached: (0, false),
            s_reclaimable: (0, false),
            shmem: (0, false),
            swap_total: (0, false),
            swap_free: (0, false)
        }
    }
    fn done(&self) -> bool {
        self.mem_total.1 &&
            self.mem_free.1 &&
            self.buffers.1 &&
            self.cached.1 &&
            self.s_reclaimable.1 &&
            self.shmem.1 &&
            self.swap_total.1 &&
            self.swap_free.1
    }
}

#[derive(Clone, Debug)]
pub struct Memory {
    name: String,
    memtype: Memtype,
    output: (ButtonWidget, ButtonWidget),
    clickable: bool,
    format: (FormatTemplate, FormatTemplate),
    update_interval: Duration,
    tx_update_request: Sender<Task>,
    values: HashMap<String, String>,
    warning: (f64,f64),
    critical: (f64,f64)
}


impl Memory {
    fn format_insert_values(&mut self, mem_state: Memstate) -> String {

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


        self.values.insert("{MTg}".to_string(), format!("{:.1}", mem_total.gib()));
        self.values.insert("{MTm}".to_string(), format!("{}", mem_total.mib()));
        self.values.insert("{MFg}".to_string(), format!("{:.1}", mem_free.gib()));
        self.values.insert("{MFm}".to_string(), format!("{}", mem_free.mib()));
        self.values.insert("{MFp}".to_string(), format!("{:.2}", mem_free.percent(mem_total)));
        self.values.insert("{MUg}".to_string(), format!("{:.1}", mem_total_used.gib()));
        self.values.insert("{MUm}".to_string(), format!("{}", mem_total_used.mib()));
        self.values.insert("{MUp}".to_string(), format!("{:.2}", mem_total_used.percent(mem_total)));
        self.values.insert("{Mug}".to_string(), format!("{:.1}", mem_used.gib()));
        self.values.insert("{Mum}".to_string(), format!("{}", mem_used.mib()));
        self.values.insert("{Mup}".to_string(), format!("{:.2}", mem_used.percent(mem_total)));
        self.values.insert("{MAg}".to_string(), format!("{:.1}", mem_avail.gib()));
        self.values.insert("{MAm}".to_string(), format!("{}", mem_avail.mib()));
        self.values.insert("{MAp}".to_string(), format!("{:.2}", mem_avail.percent(mem_total)));
        self.values.insert("{STg}".to_string(), format!("{:.1}", swap_total.gib()));
        self.values.insert("{STm}".to_string(), format!("{}", swap_total.mib()));
        self.values.insert("{SFg}".to_string(), format!("{:.1}", swap_free.gib()));
        self.values.insert("{SFm}".to_string(), format!("{}", swap_free.mib()));
        self.values.insert("{SFp}".to_string(), format!("{:.2}", swap_free.percent(swap_total)));
        self.values.insert("{SUg}".to_string(), format!("{:.1}", swap_used.gib()));
        self.values.insert("{SUm}".to_string(), format!("{}", swap_used.mib()));
        self.values.insert("{SUp}".to_string(), format!("{:.2}", swap_used.percent(swap_total)));
        self.values.insert("{Bg}".to_string(), format!("{:.1}", buffers.gib()));
        self.values.insert("{Bm}".to_string(), format!("{}", buffers.mib()));
        self.values.insert("{Bp}".to_string(), format!("{:.2}", buffers.percent(mem_total)));
        self.values.insert("{Cg}".to_string(), format!("{:.1}", cached.gib()));
        self.values.insert("{Cm}".to_string(), format!("{}", cached.mib()));
        self.values.insert("{Cp}".to_string(), format!("{:.2}", cached.percent(mem_total)));




        match self.memtype {
            Memtype::MEMORY => self.output.0.set_state(
                match mem_used.percent(mem_total) {
                    x if x as f64 > self.critical.0 => State::Critical,
                    x if x as f64 > self.warning.0 => State::Warning,
                    _ => State::Idle,
                }
            ),
            Memtype::SWAP => self.output.1.set_state(
                match swap_used.percent(swap_total) {
                    x if x as f64 > self.critical.1 => State::Critical,
                    x if x as f64 > self.warning.1 => State::Warning,
                    _ => State::Idle,
                }
            )
        };

        if_debug!({
            let mut f = OpenOptions::new().create(true).append(true).open("/tmp/i3log").unwrap();
            writeln!(f, "Inserted values: {:?}", self.values);
        });

        match self.memtype {
            Memtype::MEMORY => self.format.0.render(&self.values),
            Memtype::SWAP => self.format.1.render(&self.values)
        }
    }


    pub fn switch(&mut self) {

        let old: Memtype = self.memtype.clone();
        self.memtype = match old {
            Memtype::MEMORY => Memtype::SWAP,
            _ => Memtype::MEMORY
        };
    }
    pub fn new(config: Value, tx: Sender<Task>, theme: Value) -> Memory {
        let memtype: String = get_str_default!(config, "type", "swap");
        let icons: bool = get_bool_default!(config, "icons", true);
        let widget = ButtonWidget::new(theme.clone(), "memory").with_text("");
        let memory = Memory {
            name: Uuid::new_v4().simple().to_string(),
            memtype: match memtype.as_ref() {
                "memory" => Memtype::MEMORY,
                "swap" => Memtype::SWAP,
                _ => panic!(format!("Invalid Memory type: {}", memtype))
            },
            output:
            if icons {
                (widget.clone().with_icon("memory_mem"), widget.with_icon("memory_swap"))
            } else {
                (widget.clone(), widget)
            }
            ,
            clickable: get_bool_default!(config, "clickable", true),
            format: (match config["format_mem"] {
                Value::String(ref e) => {
                    FormatTemplate::from_string(e.clone()).unwrap()
                }
                _ => FormatTemplate::from_string("{Mum}MB/{MTm}MB({Mup}%)".to_string()).unwrap()
            }, match config["format_swap"] {
                Value::String(ref e) => {
                    FormatTemplate::from_string(e.clone()).unwrap()
                }
                _ => FormatTemplate::from_string("{SUm}MB/{STm}MB({SUp}%)".to_string()).unwrap()
            }
            ),
            update_interval: Duration::new(get_u64_default!(config, "interval", 5), 0),
            tx_update_request: tx,
            values: HashMap::<String, String>::new(),
            warning: (get_f64_default!(config, "warning_mem", 80f64), get_f64_default!(config, "warning_swap", 80f64)),
            critical: (get_f64_default!(config, "critical_mem", 95f64), get_f64_default!(config, "critical_swap", 95f64)),
        };
        memory

    }
}


impl Block for Memory
{
    fn id(&self) -> &str {
        &self.name
    }

    fn update(&mut self) -> Option<Duration> {

        let f = File::open("/proc/meminfo").expect("/proc/meminfo does not exist. \
                                                    Are we on a linux system?");
        let f = BufReader::new(f);

        let mut mem_state = Memstate::new();


        for line in f.lines() {
            if_debug!({
            let mut f = OpenOptions::new().create(true).append(true).open("/tmp/i3log").unwrap();
            writeln!(f, "Updated: {:?}", mem_state);
        });
            // stop reading if all values are already present
            if mem_state.done() {
                break
            }

            let line = match line {
                Ok(s) => s,
                _ => { continue }
            };
            let line = line.split_whitespace().collect::<Vec<&str>>();


            match line[0] {
                "MemTotal:" => {
                    mem_state.mem_total = (u64::from_str(line[1]).unwrap(), true);
                    continue;
                }
                "MemFree:" => {
                    mem_state.mem_free = (u64::from_str(line[1]).unwrap(), true);
                    continue;
                }
                "Buffers:" => {
                    mem_state.buffers = (u64::from_str(line[1]).unwrap(), true);
                    continue;
                }
                "Cached:" => {
                    mem_state.cached = (u64::from_str(line[1]).unwrap(), true);
                    continue;
                }
                "SReclaimable:" => {
                    mem_state.s_reclaimable = (u64::from_str(line[1]).unwrap(), true);
                    continue;
                }
                "Shmem:" => {
                    mem_state.shmem = (u64::from_str(line[1]).unwrap(), true);
                    continue;
                }
                "SwapTotal:" => {
                    mem_state.swap_total = (u64::from_str(line[1]).unwrap(), true);
                    continue;
                }
                "SwapFree:" => {
                    mem_state.swap_free = (u64::from_str(line[1]).unwrap(), true);
                    continue;
                }
                _ => { continue; }
            }
        }

        // Now, create the string to be shown
        let output_text = self.format_insert_values(mem_state);

        match self.memtype {
            Memtype::MEMORY => self.output.0.set_text(output_text),
            Memtype::SWAP => self.output.1.set_text(output_text),
        }

        if_debug!({
            let mut f = OpenOptions::new().create(true).append(true).open("/tmp/i3log").unwrap();
            writeln!(f, "Updated: {:?}", self);
        });
        Some(self.update_interval.clone())
    }


    fn click_left(&mut self, event: &I3barEvent) {

        if_debug!({
            let mut f = OpenOptions::new().create(true).append(true).open("/tmp/i3log").unwrap();
            writeln!(f, "Click received: {:?}", event);
        });

        if let Some(ref s) = event.name {
            if self.clickable && *s == "memory".to_string() {
                self.switch();
                self.update();
                self.tx_update_request.send(Task {
                    id: self.name.clone(),
                    update_time: Instant::now()
                }).ok();
            }
        }


    }

    fn view(&self) -> Vec<&I3BarWidget> {
        vec![match self.memtype {
            Memtype::MEMORY => &self.output.0,
            Memtype::SWAP => &self.output.1,
        }]
    }
}

//! ##Memory
//! Creates a block displaying memory and swap usage.
//!
//! By default, the format of this module is "<Icon>: {MFm}MB/{MTm}MB({Mp}%)" (Swap values
//! accordingly). That behaviour can be changed within config.json.
//!
//! This module keeps track of both Swap and Memory. By default, a click switches between them.
//!
//! **Example**
//! ```javascript
//! {"block": "memory",
//!     "format_mem": "{MFm}MB/{MTm}MB({Mp}%)", "format_swap": "{SFm}MB/{STm}MB({Sp}%)",
//!     "type": "memory", "icons": "true", "clickable": "true", "interval": "5"
//! },
//! ```
//!
//! **Options**
//! Key | Values | Required | Default
//! ----|--------|----------|--------
//! format_mem | Format string for Memory view. All format values are described below. | No | {MFm}MB/{MTm}MB({Mp}%)
//! format_swap | Format string for Swap view. | No | {SFm}MB/{STm}MB({Sp}%)
//! type | Default view displayed on startup. Options are <br/> memory, swap | No | memory
//! icons | Whether the format string should be prepended with Icons. Options are <br/> true, false | No | true
//! clickable | Whether the view should switch between memory and swap on click. Options are <br/> true, false | No | true
//! interval | The delay in seconds between an update. If `clickable`, an update is triggered on click. Integer values only. | No | 5
//!
//! ###Format string specification
//! Key | Values
//! ----|-------
//! {MTg} | Memory total (GiB)
//! {MTm} | Memory total (MiB)
//! {MFg} | Memory free (GiB)
//! {MFm} | Memory free (MiB)
//! {Mp} | Memory used (%)
//! {STg} | Swap total (GiB)
//! {STm} | Swap total (MiB)
//! {SFg} | Swap free (GiB)
//! {SFm} | Swap free (MiB)
//! {Sp} | Swap used (%)
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


use widgets::text::TextWidget;
use widget::I3BarWidget;
use scheduler::Task;


#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Memtype {
    SWAP,
    MEMORY
}

#[derive(Clone, Debug)]
pub struct Memory {
    name: String,
    memtype: Memtype,
    output: HashMap<Memtype, TextWidget>,
    clickable: bool,
    format: HashMap<Memtype, FormatTemplate>,
    update_interval: Duration,
    tx_update_request: Sender<Task>,
    values: HashMap<String, String>
}


impl Memory {
    /// mem_state: (memory_total, memory_free, swap_total, swap_free)
    ///
    fn format_insert_values(&mut self, mem_state: (usize, usize, usize, usize)) -> String {
        let mtg: usize = mem_state.0 / (1024 * 1024);
        let mtm: usize = mem_state.0 / 1024;
        let mfg: usize = mem_state.1 / (1024 * 1024);
        let mfm: usize = mem_state.1 / 1024;
        let mp: f32 = 100f32 - 100f32 * (mem_state.1 as f32) / (mem_state.0 as f32);
        let stg: usize = mem_state.2 / (1024 * 1024);
        let stm: usize = mem_state.2 / 1024;
        let sfg: usize = mem_state.3 / (1024 * 1024);
        let sfm: usize = mem_state.3 / 1024;
        let sp: f32 = 100f32 - 100f32 * (mem_state.3 as f32) / (mem_state.2 as f32);

        self.values.insert("{MTg}".to_string(), format!("{}", mtg));
        self.values.insert("{MTm}".to_string(), format!("{}", mtm));
        self.values.insert("{MFg}".to_string(), format!("{}", mfg));
        self.values.insert("{MFm}".to_string(), format!("{}", mfm));
        self.values.insert("{Mp}".to_string(), format!("{:.2}", mp));
        self.values.insert("{STg}".to_string(), format!("{}", stg));
        self.values.insert("{STm}".to_string(), format!("{}", stm));
        self.values.insert("{SFg}".to_string(), format!("{}", sfg));
        self.values.insert("{SFm}".to_string(), format!("{}", sfm));
        self.values.insert("{Sp}".to_string(), format!("{:.2}", sp));
        self.format.get(&self.memtype).unwrap().render(&self.values)
    }


    pub fn switch(&mut self) {
        let old: Memtype = self.memtype.clone();
        self.memtype = match old {
            Memtype::MEMORY => Memtype::SWAP,
            _ => Memtype::MEMORY
        };
    }
    pub fn new(config: Value, tx: Sender<Task>, theme: Value) -> Memory {
        let memtype: String = get_str_default!(config, "type", "memory");
        let icons: bool = get_bool_default!(config, "icons", true);
        let textwidget = TextWidget::new(theme.clone()).with_text("");
        Memory {
            name: Uuid::new_v4().simple().to_string(),
            memtype: match memtype.as_ref() {
                "memory" => Memtype::MEMORY,
                "swap" => Memtype::SWAP,
                _ => panic!(format!("Invalid Memory type: {}", memtype))
            },
            output:
            if icons {
                map!(Memtype::SWAP => textwidget.clone().with_icon("memory_swap"),
                         Memtype::MEMORY => textwidget.with_icon("memory_mem"))
            } else {
                map!(Memtype::SWAP => textwidget.clone(),
                         Memtype::MEMORY => textwidget)
            }
            ,
            clickable: get_bool_default!(config, "clickable", true),
            format: map!(
            Memtype::MEMORY => match config["format_mem"] {
                Value::String(ref e) => {
                    FormatTemplate::from_string(e.clone()).unwrap()
                },
                _ =>FormatTemplate::from_string("{MFm}MB/{MTm}MB({Mp}%)".to_string()).unwrap()
            }, Memtype::SWAP => match config["format_swap"] {
                Value::String(ref e) => {
                    FormatTemplate::from_string(e.clone()).unwrap()
                },
                _ =>FormatTemplate::from_string("{SFm}MB/{STm}MB({Sp}%)".to_string()).unwrap()
            }
            ),
            update_interval: Duration::new(get_u64_default!(config, "interval", 5), 0),
            tx_update_request: tx,
            values: map!(
                "{MTg}".to_string()=> "".to_string(),
                "{MTm}".to_string()=> "".to_string(),
                "{MFg}".to_string()=> "".to_string(),
                "{MFm}".to_string()=> "".to_string(),
                "{Mp}".to_string()=> "".to_string(),

                "{STg}".to_string()=>"".to_string(),
                "{STm}".to_string()=>"".to_string(),
                "{SFg}".to_string()=> "".to_string(),
                "{SFm}".to_string()=> "".to_string(),
                "{Sp}".to_string()=> "".to_string()
            ),
        }
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

        let (mut mt, mut mf, mut st, mut sf) = (false, false, false, false);

        let mut mem_state: (usize, usize, usize, usize) = (0, 0, 0, 0);

        let lines = f.lines();
        for line in lines {
            // stop reading if all four values are already present
            if mt && mf && st && sf {
                break
            }

            let line = match line {
                Ok(s) => s,
                _ => { continue }
            };
            let line: Vec<&str> = line.split_whitespace().collect::<Vec<&str>>();


            match line[0] {
                "MemTotal:" => {
                    mt = true;
                    mem_state.0 = usize::from_str(line[1]).unwrap();
                    continue;
                }
                "MemFree:" => {
                    mf = true;
                    mem_state.1 = usize::from_str(line[1]).unwrap();
                    continue;
                }
                "SwapTotal:" => {
                    st = true;
                    mem_state.2 = usize::from_str(line[1]).unwrap();
                    continue;
                }
                "SwapFree:" => {
                    sf = true;
                    mem_state.3 = usize::from_str(line[1]).unwrap();
                    continue;
                }
                _ => { continue; }
            }
        }
        if !(mt && mf && st && sf) {
            panic!("/proc/meminfo does not contain valid usage information for swap and memory.")
        }

        // Now, create the string to be shown

        let output_text = self.format_insert_values(mem_state);
        self.output.get_mut(&self.memtype).unwrap().set_text(output_text);

        Some(self.update_interval.clone())
    }


    fn click(&mut self, event: &I3barEvent) {
        if self.clickable && event.button == 1 {
            self.switch();
            self.update();
            self.tx_update_request.send(Task { id: self.name.clone(), update_time: Instant::now() }).ok();
        }
    }

    fn view(&self) -> Vec<&I3BarWidget> {
        vec![self.output.get(&self.memtype).unwrap()]
    }
}

extern crate sysinfo;

use std::cell::Cell;
use std::time::Duration;
use serde_json::Value;
use block::Block;
use self::sysinfo::{ProcessorExt, System, SystemExt};

pub struct Cpu {
    usage: Cell<f32>,
    name: &'static str,
}

impl Cpu {
    pub fn new(name: &'static str) -> Cpu {
        Cpu {
            usage: Cell::new(0.),
            name: name,
        }
    }
}


impl Block for Cpu
{
    fn id(&self) -> Option<&str> {
        Some(self.name)
    }

    fn update(&self) -> Option<Duration> {
        self.usage.set(System::new().get_processor_list()[0].get_cpu_usage());
        Some(Duration::new(5, 0))
    }

    fn get_status(&self, _: &Value) -> Value {
        json!({
            "full_text" : format!("{}%", self.usage.get())
        })
    }
}
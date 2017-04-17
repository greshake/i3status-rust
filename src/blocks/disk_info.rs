extern crate nix;

use std::cell::Cell;
use std::path::Path;
use std::time::Duration;

use block::{Block, State};

use self::nix::sys::statvfs::vfs::Statvfs;
use serde_json::Value;

pub enum Unit {
    MB,
    GB,
    GiB,
    MiB
}

impl Unit {
    fn convert_bytes(&self, bytes: u64) -> f64 {
        use self::Unit::*;
        match *self {
            MB => bytes as f64 / 1000. / 1000.,
            GB => bytes as f64 / 1000. / 1000. / 1000.,
            MiB => bytes as f64 / 1024. / 1024.,
            GiB => bytes as f64 / 1024. / 1024. / 1024.,
        }
    }

    fn name(&self) -> &'static str {
        use self::Unit::*;
        match *self {
            MB => "MB",
            GB => "GB",
            MiB => "MiB",
            GiB => "GiB"
        }
    }
}

pub struct DiskInfo
{
    alias: &'static str,
    target: &'static str,
    value: Cell<f64>,
    info_type: DiskInfoType,
    unit: Unit,

}

pub enum DiskInfoType {
    Available,
    Free,
    Total,
    Used,
}

impl DiskInfoType {
    fn from_str(name: &str) -> DiskInfoType {
        use self::DiskInfoType::*;
        match name {
            "available" => Available,
            "free" => Free,
            "total" => Total,
            "used" => Used,
            _ => panic!("Invalid DiskInfo Type")
        }
    }
}

impl DiskInfo
{
    pub fn new(target: &'static str, alias: &'static str, info_type: DiskInfoType, unit: Unit) -> DiskInfo
    {
        DiskInfo
            {
                alias: alias,
                target: target,
                value: Cell::new(0.),
                info_type: info_type,
                unit: unit,
            }
    }
}

impl Block for DiskInfo
{
    fn update(&self) -> Option<Duration> {
        match self.info_type {
            DiskInfoType::Available => {
                let statvfs = Statvfs::for_path(Path::new(self.target)).unwrap();
                let available = self.unit.convert_bytes(statvfs.f_bavail * statvfs.f_bsize);
                self.value.set(available);
            }
            DiskInfoType::Free => {
                let statvfs = Statvfs::for_path(Path::new(self.target)).unwrap();
                let free = self.unit.convert_bytes(statvfs.f_bfree * statvfs.f_bsize);
                self.value.set(free);
            }
            _ => unimplemented!(),
        }
        Some(Duration::new(5, 0))
    }

    fn get_status(&self, _: &Value) -> Value {
        match self.info_type {
            DiskInfoType::Available => {
                json!({"full_text" : format!(" {0} avail: {1:.2} {2} ", self.alias, self.value.get(),
                self.unit.name())})
            }
            DiskInfoType::Free => {
                json!({"full_text" : format!(" {0} free: {1:.2} {2} ", self.alias, self.value.get(),
                self.unit.name())})
            }
            _ => unimplemented!(),
        }
    }

    fn get_state(&self) -> State {
        match self.info_type {
            DiskInfoType::Available | DiskInfoType::Free => {
                // This could cause trouble: https://github.com/rust-lang/rust/issues/41255
                match self.value.get() {
                    0. ... 10. => State::Critical,
                    10. ... 20. => State::Warning,
                    _ => State::Good,
                }
            }
            _ => unimplemented!(),
        }
    }
}
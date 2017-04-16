extern crate nix;

use std::cell::{Cell, RefCell};
use std::time::Duration;
use std::path::Path;
use serde_json::Value;
use block::{Block, State};

use self::nix::sys::statvfs::vfs::Statvfs;


pub enum Unit {
    MB, GB, GiB, MiB
}

impl Unit {
    fn convert_bytes(&self, bytes: u64) -> f64 {
        use self::Unit::*;
        match *self {
            MB  => bytes as f64 / 1000. / 1000.,
            GB  => bytes as f64 / 1000. / 1000. / 1000.,
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

pub struct DiskUsage
{
    alias: &'static str,
    target: &'static str,
    free: Cell<u64>,
    total: Cell<u64>,
    unit: Unit,
    statvfs: RefCell<Statvfs>,
}

impl DiskUsage
{
    pub fn new(target: &'static str, alias: &'static str, unit: Unit) -> DiskUsage
    {
        DiskUsage
            {
                alias: alias,
                target: target,
                free: Cell::new(0),
                total: Cell::new(0),
                unit: unit,
                statvfs: RefCell::new(Statvfs::default()),
            }
    }
}

impl Block for DiskUsage
{
    fn update(&self) -> Option<Duration> {
        let path = Path::new(self.target);
        let statvfs = &mut *self.statvfs.borrow_mut();

        statvfs.update_with_path(path);
        let free = (statvfs.f_bsize  * statvfs.f_bfree) as u64;
        let total = (statvfs.f_frsize * statvfs.f_blocks) as u64;
        self.free.set(free);
        self.total.set(total);

        Some(Duration::new(5, 0))
    }

    fn get_status(&self, _: &Value) -> Value {
        let free = self.unit.convert_bytes(self.free.get());
        let total = self.unit.convert_bytes(self.total.get());

        json!({
            "full_text" : format!(" {perc:.0}% ({free:.0}{unit}) free on {alias} ",
                                    alias=self.alias,
                                    free=free,
                                     unit=self.unit.name(),
                                      perc=(free / total) * 100.)
        })
    }

    fn get_state(&self) -> State {
        match self.free.get() as f64 / self.total.get() as f64 {
            0. ...0.1 => State::Critical,
            0.1 ...0.2 => State::Warning,
            _ => State::Good
        }
    }
}
extern crate nix;

use std::cell::{Cell, RefCell};
use std::time::Duration;
use std::path::Path;
use serde_json::Value;
use block::{Block, State};

use self::nix::sys::statvfs::vfs::Statvfs;

pub struct DiskUsage
{
    alias: &'static str,
    target: &'static str,
    free: Cell<f64>,
    statvfs: RefCell<Statvfs>,

}

impl DiskUsage
{
    pub fn new(target: &'static str, alias: &'static str) -> DiskUsage
    {
        DiskUsage
            {
                alias: alias,
                target: target,
                free: Cell::new(0.),
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
        let free: f64 = (statvfs.f_bsize  * statvfs.f_bfree) as f64;
        self.free.set(free / (1024 * 1024 * 1024) as f64);

        Some(Duration::new(5, 0))
    }

    fn get_status(&self, _: &Value) -> Value {
        json!({
            "full_text" : format!(" {0} {1}GB ", self.alias, self.free.get())
        })
    }

    fn get_state(&self) -> State {
        match self.free.get() {
            0. ...10. => State::Critical,
            10. ...20. => State::Warning,
            _ => State::Good
        }
    }
}
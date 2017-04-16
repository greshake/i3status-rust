extern crate nix;

use std::cell::{Cell};
use std::time::Duration;
use std::path::Path;
use serde_json::Value;
use block::{Block, State};

use self::nix::sys::statvfs::vfs::Statvfs;

const BYTES_PER_BG: f64 = 1073741824.0;

pub struct DiskInfo
{
    alias: &'static str,
    target: &'static str,
    value: Cell<f64>,
    info_type: DiskInfoType,
    state: Cell<State>,

}

pub enum DiskInfoType {
    Free,
    Used,
    Total,
    Available,
    PercentageFree,
    PercentageUsedOfAvailable,
    PercentageUsed,
    PercentageAvailable,
}

impl DiskInfo
{
    pub fn new(target: &'static str, alias: &'static str, info_type: DiskInfoType) -> DiskInfo
    {
        DiskInfo
            {
                alias: alias,
                target: target,
                value: Cell::new(0.),
                info_type: info_type,
                state: Cell::new(State::Idle),
            }
    }

    fn compute_value(&self) {
        match self.info_type {
            DiskInfoType::Free => {
                let statvfs = Statvfs::for_path(Path::new(self.target)).unwrap();

                let free = (statvfs.f_bsize * statvfs.f_bfree) as f64 / BYTES_PER_BG;
                self.value.set(free);

                // This could cause trouble: https://github.com/rust-lang/rust/issues/41255
                let new_state = match free {
                    0. ... 10. => State::Critical,
                    10. ... 20. => State::Warning,
                    _ => State::Good
                };

                self.state.set(new_state);
            }
            _ => unimplemented!(),
        }
    }
}

impl Block for DiskInfo
{
    #[inline]
    fn update(&self) -> Option<Duration> {
        self.compute_value();
        Some(Duration::new(5, 0))
    }

    fn get_status(&self, _: &Value) -> Value {
        match self.info_type {
            DiskInfoType::Free => {
                json!({"full_text" : format!(" {0} {1:.2}GB ", self.alias, self.value.get())})
            }
            _ => unimplemented!(),
        }
    }

    #[inline]
    fn get_state(&self) -> State {
        self.state.get()
    }
}
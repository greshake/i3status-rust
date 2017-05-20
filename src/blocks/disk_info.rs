use std::time::Duration;
use std::path::Path;

use block::Block;
use input::I3barEvent;
use widgets::text::TextWidget;
use widget::{I3BarWidget, State};

use serde_json::Value;
use uuid::Uuid;

extern crate nix;

use self::nix::sys::statvfs::vfs::Statvfs;

#[derive(Copy, Clone)]
pub enum Unit {
    MB,
    GB,
    GiB,
    MiB
}

impl Unit {
    fn bytes_to_unit(unit: Unit, bytes: u64) -> f64 {
        match unit {
            Unit::MB => bytes as f64 / 1000. / 1000.,
            Unit::GB => bytes as f64 / 1000. / 1000. / 1000.,
            Unit::MiB => bytes as f64 / 1024. / 1024.,
            Unit::GiB => bytes as f64 / 1024. / 1024. / 1024.,
        }
    }

    fn name(&self) -> &'static str {
        match *self {
            Unit::MB => "MB",
            Unit::GB => "GB",
            Unit::MiB => "MiB",
            Unit::GiB => "GiB"
        }
    }

    fn from_str(name: &str) -> Unit {
        match name {
            "MB" => Unit::MB,
            "GB" => Unit::GB,
            "MiB" => Unit::MiB,
            "GiB" => Unit::GiB,
            _ => panic!("Invalid unit name"),
        }
    }
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
            "total" => unimplemented!(), //Total,
            "used" => unimplemented!(), // Used,
            _ => panic!("Invalid DiskInfo Type")
        }
    }
}

pub struct DiskInfo {
    info: TextWidget,
    id: String,
    update_interval: Duration,
    alias: String,
    target: String,
    info_type: DiskInfoType,
    unit: Unit,

}

impl DiskInfo {
    pub fn new(config: Value, theme: Value) -> DiskInfo {
        {
            DiskInfo {
                id: Uuid::new_v4().simple().to_string(),
                update_interval: Duration::new(get_u64_default!(config, "interval", 10), 0),
                info: TextWidget::new(theme.clone()).with_text("DiskInfo"),
                alias: get_str_default!(config, "alias", "/"),
                target: get_str_default!(config, "target","/"),
                info_type: DiskInfoType::from_str(get_str_default!(config, "type", "free").as_str()),
                unit: Unit::from_str(get_str_default!(config, "unit", "GB").as_str()),
            }
        }
    }
    fn compute_state(&self, bytes: u64) -> State {
        let value = Unit::bytes_to_unit(Unit::GB, bytes);
        match self.info_type {
            DiskInfoType::Available | DiskInfoType::Free => {
                if 0. <= value && value < 10. {
                    State::Critical
                } else if 10. <= value && value < 20. {
                    State::Warning
                } else { State::Good }
            }
            DiskInfoType::Total | DiskInfoType::Used => unimplemented!(),
        }
    }
}


impl Block for DiskInfo
{
    fn update(&mut self) -> Option<Duration> {
        match self.info_type {
            DiskInfoType::Available => {
                let statvfs = Statvfs::for_path(Path::new(self.target.as_str())).unwrap();
                let available_bytes = statvfs.f_bavail * statvfs.f_bsize;

                let available = Unit::bytes_to_unit(self.unit, available_bytes);
                self.info.set_text(format!("{0} {1:.2} {2}", self.alias, available, self.unit.name()));

                let state = self.compute_state(available_bytes);
                self.info.set_state(state);
            }
            DiskInfoType::Free => {
                let statvfs = Statvfs::for_path(Path::new(self.target.as_str())).unwrap();
                let free_bytes = statvfs.f_bfree * statvfs.f_bsize;

                let free = Unit::bytes_to_unit(self.unit, free_bytes);
                self.info.set_text(format!("{0} {1:.2} {2}", self.alias, free, self.unit.name()));

                let state = self.compute_state(free_bytes);
                self.info.set_state(state);
            }
            DiskInfoType::Total | DiskInfoType::Used => unimplemented!(),
        }

        Some(self.update_interval.clone())
    }

    fn view(&self) -> Vec<&I3BarWidget> {
        vec![&self.info]
    }
    fn click(&mut self, _: &I3barEvent) {}
    fn id(&self) -> &str {
        &self.id
    }
}

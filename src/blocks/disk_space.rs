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
    fn bytes_in_unit(unit: Unit, bytes: u64) -> f64 {
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


pub enum InfoType {
    Available,
    Free,
    Total,
    Used,
}

impl InfoType {
    fn from_str(name: &str) -> InfoType {
        match name {
            "available" => InfoType::Available,
            "free" => InfoType::Free,
            "total" => unimplemented!(), // SpaceType::Total,
            "used" => unimplemented!(), // SpaceType::Used,
            _ => panic!("Invalid InfoType")
        }
    }
}

pub struct DiskSpace {
    disk_space: TextWidget,
    id: String,
    update_interval: Duration,
    alias: String,
    path: String,
    info_type: InfoType,
    unit: Unit,
}

impl DiskSpace {
    pub fn new(config: Value, theme: Value) -> DiskSpace {
        DiskSpace {
            id: Uuid::new_v4().simple().to_string(),
            update_interval: Duration::new(get_u64_default!(config, "interval", 20), 0),
            disk_space: TextWidget::new(theme.clone()).with_text("DiskSpace"),
            alias: get_str_default!(config, "alias", "/"),
            path: get_str_default!(config, "path","/"),
            info_type: InfoType::from_str(get_str_default!(config, "type", "available").as_str()),
            unit: Unit::from_str(get_str_default!(config, "unit", "GB").as_str()),
        }
    }

    fn compute_state(&self, bytes: u64) -> State {
        let value = Unit::bytes_in_unit(Unit::GB, bytes);
        match self.info_type {
            InfoType::Available | InfoType::Free => {
                if 0. <= value && value < 10. {
                    State::Critical
                } else if 10. <= value && value < 20. {
                    State::Warning
                } else { State::Idle }
            }
            InfoType::Total | InfoType::Used => unimplemented!(),
        }
    }
}


impl Block for DiskSpace {
    fn update(&mut self) -> Option<Duration> {
        let statvfs = Statvfs::for_path(Path::new(self.path.as_str())).unwrap();
        let result;
        let converted;

        match self.info_type {
            InfoType::Available => {
                result = statvfs.f_bavail * statvfs.f_bsize;
                converted = Unit::bytes_in_unit(self.unit, result);
            }
            InfoType::Free => {
                result = statvfs.f_bfree * statvfs.f_bsize;
                converted = Unit::bytes_in_unit(self.unit, result);
            }
            InfoType::Total | InfoType::Used => unimplemented!(),
        }

        self.disk_space.set_text(format!("{0} {1:.2} {2}", self.alias, converted, self.unit.name()));

        let state = self.compute_state(result);
        self.disk_space.set_state(state);

        Some(self.update_interval.clone())
    }

    fn view(&self) -> Vec<&I3BarWidget> {
        vec![&self.disk_space]
    }
    fn click_left(&mut self, _: &I3barEvent) {}
    fn id(&self) -> &str {
        &self.id
    }
}

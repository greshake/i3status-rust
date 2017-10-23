use std::time::Duration;
use std::path::Path;
use chan::Sender;
use scheduler::Task;

use block::{Block, ConfigBlock};
use config::Config;
use de::deserialize_duration;
use errors::*;
use widgets::text::TextWidget;
use widget::{I3BarWidget, State};

use uuid::Uuid;

extern crate nix;

use self::nix::sys::statvfs::vfs::Statvfs;

#[derive(Copy, Clone)]
pub enum Unit {
    MB,
    GB,
    GiB,
    MiB,
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
            Unit::GiB => "GiB",
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
    // TODO: implement
    //Total,
    //Used,
}

impl InfoType {
    fn from_str(name: &str) -> InfoType {
        match name {
            "available" => InfoType::Available,
            "free" => InfoType::Free,
            "total" => unimplemented!(), // SpaceType::Total,
            "used" => unimplemented!(), // SpaceType::Used,
            _ => panic!("Invalid InfoType"),
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
    warning: f64,
    alert: f64,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct DiskSpaceConfig {
    /// Path to collect information from
    #[serde(default = "DiskSpaceConfig::default_path")]
    pub path: String,

    /// Alias that is displayed for path
    #[serde(default = "DiskSpaceConfig::default_alias")]
    pub alias: String,

    /// Currently supported options are available and free
    #[serde(default = "DiskSpaceConfig::default_info_type")]
    pub info_type: String,

    /// Unit that is used to display disk space. Options are MB, MiB, GB and GiB
    #[serde(default = "DiskSpaceConfig::default_unit")]
    pub unit: String,

    /// Update interval in seconds
    #[serde(default = "DiskSpaceConfig::default_interval", deserialize_with = "deserialize_duration")]
    pub interval: Duration,

    /// Diskspace warning in GiB (yellow)
    #[serde(default = "DiskSpaceConfig::default_warning")]
    pub warning: f64,

    /// Diskspace alert in GiB (red)
    #[serde(default = "DiskSpaceConfig::default_alert")]
    pub alert: f64,

}

impl DiskSpaceConfig {
    fn default_path() -> String {
        "/".to_owned()
    }

    fn default_alias() -> String {
        "/".to_owned()
    }

    fn default_info_type() -> String {
        "available".to_owned()
    }

    fn default_unit() -> String {
        "GB".to_owned()
    }

    fn default_interval() -> Duration {
        Duration::from_secs(20)
    }

    fn default_warning() -> f64 {
        20.
    }

    fn default_alert() -> f64 {
        10.
    }
}

impl DiskSpace {
    fn compute_state(&self, bytes: u64, warning: f64, alert: f64) -> State {
        let value = Unit::bytes_in_unit(Unit::GB, bytes);
        match self.info_type {
            InfoType::Available | InfoType::Free => if 0. <= value && value < alert {
                State::Critical
            } else if alert <= value && value < warning {
                State::Warning
            } else {
                State::Idle
            },
            //InfoType::Total | InfoType::Used => unimplemented!(),
        }
    }
}

impl ConfigBlock for DiskSpace {
    type Config = DiskSpaceConfig;

    fn new(block_config: Self::Config, config: Config, _tx_update_request: Sender<Task>) -> Result<Self> {
        Ok(DiskSpace {
            id: Uuid::new_v4().simple().to_string(),
            update_interval: block_config.interval,
            disk_space: TextWidget::new(config).with_text("DiskSpace"),
            alias: block_config.alias,
            path: block_config.path,
            info_type: InfoType::from_str(&block_config.info_type),
            unit: Unit::from_str(&block_config.unit),
            warning: block_config.warning,
            alert: block_config.alert,
        })
    }
}

impl Block for DiskSpace {
    fn update(&mut self) -> Result<Option<Duration>> {
        let statvfs = Statvfs::for_path(Path::new(self.path.as_str()))
            .block_error("disk_space", "failed to retrieve statvfs")?;
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
            //InfoType::Total | InfoType::Used => unimplemented!(),
        }

        self.disk_space.set_text(format!(
            "{0} {1:.2} {2}",
            self.alias,
            converted,
            self.unit.name()
        ));

        let state = self.compute_state(result, self.warning, self.alert);
        self.disk_space.set_state(state);

        Ok(Some(self.update_interval))
    }

    fn view(&self) -> Vec<&I3BarWidget> {
        vec![&self.disk_space]
    }

    fn id(&self) -> &str {
        &self.id
    }
}

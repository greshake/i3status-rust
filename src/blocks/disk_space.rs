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

use self::nix::sys::statvfs::statvfs;

#[derive(Copy, Clone, Debug, Deserialize, PartialEq, Eq)]
pub enum Unit {
    MB,
    GB,
    GiB,
    MiB,
    Percent
}

impl Unit {
    fn bytes_in_unit(unit: Unit, bytes: u64) -> f64 {
        match unit {
            Unit::MB => bytes as f64 / 1000. / 1000.,
            Unit::GB => bytes as f64 / 1000. / 1000. / 1000.,
            Unit::MiB => bytes as f64 / 1024. / 1024.,
            Unit::GiB => bytes as f64 / 1024. / 1024. / 1024.,
            Unit::Percent => bytes as f64,
        }
    }
}

#[derive(Copy, Clone, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InfoType {
    Available,
    Free,
    Total,
    Used,
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
    show_percentage: bool,
}

#[derive(Deserialize, Debug, Clone)]
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
    pub info_type: InfoType,

    /// Unit that is used to display disk space. Options are MB, MiB, GB and GiB
    #[serde(default = "DiskSpaceConfig::default_unit")]
    pub unit: Unit,

    /// Update interval in seconds
    #[serde(default = "DiskSpaceConfig::default_interval", deserialize_with = "deserialize_duration")]
    pub interval: Duration,

    /// Diskspace warning in GiB (yellow)
    #[serde(default = "DiskSpaceConfig::default_warning")]
    pub warning: f64,

    /// Diskspace alert in GiB (red)
    #[serde(default = "DiskSpaceConfig::default_alert")]
    pub alert: f64,

    /// Show percentage
    #[serde(default = "DiskSpaceConfig::default_show_percentage")]
    pub show_percentage: bool,
}

impl DiskSpaceConfig {
    fn default_path() -> String {
        "/".to_owned()
    }

    fn default_alias() -> String {
        "/".to_owned()
    }

    fn default_info_type() -> InfoType {
        InfoType::Available
    }

    fn default_unit() -> Unit {
        Unit::GB
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

    fn default_show_percentage() -> bool {
        false
    }
}

impl DiskSpace {
    fn compute_state(&self, bytes: u64, warning: f64, alert: f64) -> State {
        let value = if self.unit == Unit::Percent { bytes as f64 } else { Unit::bytes_in_unit(Unit::GB, bytes) };
        match self.unit {
            Unit::Percent => {
                match self.info_type {
                    InfoType::Available | InfoType::Free | InfoType::Total | InfoType::Used => if value > alert {
                        State::Critical
                    } else if value <= alert && value > warning {
                        State::Warning
                    } else {
                        State::Idle
                    }
                }
            }
            _ => {
                match self.info_type {
                    InfoType::Available | InfoType::Free | InfoType::Total | InfoType::Used => if 0. <= value && value < alert {
                        State::Critical
                    } else if alert <= value && value < warning {
                        State::Warning
                    } else {
                        State::Idle
                    }
                }
            }
        }
    }
}

impl ConfigBlock for DiskSpace {
    type Config = DiskSpaceConfig;

    fn new(block_config: Self::Config, config: Config, _tx_update_request: Sender<Task>) -> Result<Self> {
        Ok(DiskSpace {
            id: format!("{}", Uuid::new_v4().to_simple()),
            update_interval: block_config.interval,
            disk_space: TextWidget::new(config).with_text("DiskSpace"),
            alias: block_config.alias,
            path: block_config.path,
            info_type: block_config.info_type,
            unit: block_config.unit,
            warning: block_config.warning,
            alert: block_config.alert,
            show_percentage: block_config.show_percentage,
        })
    }
}

impl Block for DiskSpace {
    fn update(&mut self) -> Result<Option<Duration>> {
        let statvfs = statvfs(Path::new(self.path.as_str()))
            .block_error("disk_space", "failed to retrieve statvfs")?;
        let mut result;
        let mut converted = 0.0f64;
        let mut converted_str = String::new();
        let total = statvfs.blocks() * statvfs.fragment_size();
        let used = (statvfs.blocks() - statvfs.blocks_free()) * statvfs.fragment_size();

        match self.info_type {
            InfoType::Available => {
                result = statvfs.blocks_available() * statvfs.block_size();
                converted = Unit::bytes_in_unit(self.unit, result);
            }
            InfoType::Free => {
                result = statvfs.blocks_free() * statvfs.block_size();
                converted = Unit::bytes_in_unit(self.unit, result);
            }
            InfoType::Total => {
                result = used;
                let converted_used = Unit::bytes_in_unit(self.unit, result);
                let converted_total = Unit::bytes_in_unit(self.unit, total);

                converted_str = format!(
                                    "{0:.2}/{1:.2}",
                                    converted_used,
                                    converted_total
                                );
            }
            InfoType::Used => {
                result = used;
                converted = Unit::bytes_in_unit(self.unit, result);
            }
        }

        let percentage = (result as f32) / (total as f32) * 100f32;
        if converted_str.is_empty() {
            converted_str = format!("{0:.2}", converted);
        }

        if self.unit == Unit::Percent {
            self.disk_space.set_text(format!("{0} {1:.2}%",
                self.alias,
                percentage
            ));
            result = percentage as u64;
        } else {
            if self.show_percentage {
                self.disk_space.set_text(format!(
                    "{0} {1} ({2:.2}%) {3:?}",
                    self.alias,
                    converted_str,
                    percentage,
                    self.unit
                ));
            } else {
                self.disk_space.set_text(format!(
                    "{0} {1} {2:?}",
                    self.alias,
                    converted_str,
                    self.unit
                ));
            }
        }

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

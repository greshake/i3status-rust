use std::path::Path;
use std::time::Duration;

use crossbeam_channel::Sender;
use nix::sys::statvfs::statvfs;
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::SharedConfig;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::formatting::FormatTemplate;
use crate::formatting::{prefix::Prefix, value::Value};
use crate::scheduler::Task;
use crate::widgets::text::TextWidget;
use crate::widgets::{I3BarWidget, State};

#[derive(Copy, Clone, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InfoType {
    Available,
    Free,
    Used,
}

pub struct DiskSpace {
    id: usize,
    disk_space: TextWidget,
    update_interval: Duration,
    path: String,
    unit: Prefix,
    info_type: InfoType,
    warning: f64,
    alert: f64,
    alert_absolute: bool,
    format: FormatTemplate,
    icon: String,

    // DEPRECATED
    // TODO remove
    alias: String,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct DiskSpaceConfig {
    /// Path to collect information from
    #[serde(default = "DiskSpaceConfig::default_path")]
    pub path: String,

    /// Currently supported options are available, free, total and used
    /// Sets value used for {percentage} calculation
    /// total is the same as used, use format to set format string for output
    #[serde(default = "DiskSpaceConfig::default_info_type")]
    pub info_type: InfoType,

    /// Format string for output
    #[serde(default = "DiskSpaceConfig::default_format")]
    pub format: String,

    /// Unit that is used to display disk space. Options are B, KB, MB, GB and TB
    #[serde(default = "DiskSpaceConfig::default_unit")]
    pub unit: String,

    /// Update interval in seconds
    #[serde(
        default = "DiskSpaceConfig::default_interval",
        deserialize_with = "deserialize_duration"
    )]
    pub interval: Duration,

    /// Diskspace warning in GiB (yellow)
    #[serde(default = "DiskSpaceConfig::default_warning")]
    pub warning: f64,

    /// Diskspace alert in GiB (red)
    #[serde(default = "DiskSpaceConfig::default_alert")]
    pub alert: f64,

    /// use absolute (unit) values for disk space alerts
    #[serde(default = "DiskSpaceConfig::default_alert_absolute")]
    pub alert_absolute: bool,

    /// Alias that is displayed for path
    // DEPRECATED
    // TODO remove
    #[serde(default = "DiskSpaceConfig::default_alias")]
    pub alias: String,
}

impl DiskSpaceConfig {
    fn default_path() -> String {
        "/".to_owned()
    }

    fn default_info_type() -> InfoType {
        InfoType::Available
    }

    fn default_format() -> String {
        "{alias} {available}".to_string()
    }

    fn default_unit() -> String {
        "GB".to_string()
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

    fn default_alert_absolute() -> bool {
        false
    }

    fn default_alias() -> String {
        "/".to_string()
    }
}

enum AlertType {
    Above,
    Below,
}

impl DiskSpace {
    fn compute_state(&self, value: f64, warning: f64, alert: f64, alert_type: AlertType) -> State {
        match alert_type {
            AlertType::Above => {
                if value > alert {
                    State::Critical
                } else if value <= alert && value > warning {
                    State::Warning
                } else {
                    State::Idle
                }
            }
            AlertType::Below => {
                if 0. <= value && value < alert {
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

impl ConfigBlock for DiskSpace {
    type Config = DiskSpaceConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        shared_config: SharedConfig,
        _tx_update_request: Sender<Task>,
    ) -> Result<Self> {
        let icon = shared_config.get_icon("disk_drive")?;

        Ok(DiskSpace {
            id,
            update_interval: block_config.interval,
            disk_space: TextWidget::new(id, 0, shared_config),
            path: block_config.path,
            format: FormatTemplate::from_string(&block_config.format)?,
            info_type: block_config.info_type,
            unit: match block_config.unit.as_str() {
                "TB" => Prefix::Tera,
                "GB" => Prefix::Giga,
                "MB" => Prefix::Mega,
                "KB" => Prefix::Kilo,
                "B" => Prefix::One,
                x => {
                    return Err(BlockError(
                        "disk_space".to_string(),
                        format!("cannot set unit to '{}'", x),
                    ))
                }
            },
            warning: block_config.warning,
            alert: block_config.alert,
            alert_absolute: block_config.alert_absolute,
            icon: icon.trim().to_string(),
            alias: block_config.alias,
        })
    }
}

impl Block for DiskSpace {
    fn update(&mut self) -> Result<Option<Update>> {
        let statvfs = statvfs(Path::new(self.path.as_str()))
            .block_error("disk_space", "failed to retrieve statvfs")?;

        let total = (statvfs.blocks() as u64) * (statvfs.fragment_size() as u64);
        let used = ((statvfs.blocks() as u64) - (statvfs.blocks_free() as u64))
            * (statvfs.fragment_size() as u64);
        let available = (statvfs.blocks_available() as u64) * (statvfs.block_size() as u64);
        let free = (statvfs.blocks_free() as u64) * (statvfs.block_size() as u64);

        let result;
        let alert_type;
        match self.info_type {
            InfoType::Available => {
                result = available;
                alert_type = AlertType::Below;
            }
            InfoType::Free => {
                result = free;
                alert_type = AlertType::Below;
            }
            InfoType::Used => {
                result = used;
                alert_type = AlertType::Above;
            }
        }

        let percentage = (result as f64) / (total as f64) * 100.;
        let values = map!(
            "percentage" => Value::from_float(percentage).percents(),
            "path" => Value::from_string(self.path.clone()),
            "total" => Value::from_float(total as f64).bytes(),
            "used" => Value::from_float(used as f64).bytes(),
            "available" => Value::from_float(available as f64).bytes(),
            "free" => Value::from_float(free as f64).bytes(),
            "icon" => Value::from_string(self.icon.to_string()),
            //TODO remove
            "alias" => Value::from_string(self.alias.clone()),
        );
        self.disk_space.set_text(self.format.render(&values)?);

        // Send percentage to alert check if we don't want absolute alerts
        let alert_val = if self.alert_absolute {
            (match self.unit {
                Prefix::Tera => result << 40,
                Prefix::Giga => result << 30,
                Prefix::Mega => result << 20,
                Prefix::Kilo => result << 10,
                Prefix::One => result,
                _ => unreachable!(),
            }) as f64
        } else {
            percentage
        };

        let state = self.compute_state(alert_val, self.warning, self.alert, alert_type);
        self.disk_space.set_state(state);

        Ok(Some(self.update_interval.into()))
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.disk_space]
    }

    fn id(&self) -> usize {
        self.id
    }
}

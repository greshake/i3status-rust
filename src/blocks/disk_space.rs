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
#[serde(deny_unknown_fields, default)]
pub struct DiskSpaceConfig {
    /// Path to collect information from
    pub path: String,

    /// Currently supported options are available, free, total and used
    /// Sets value used for {percentage} calculation
    /// total is the same as used, use format to set format string for output
    pub info_type: InfoType,

    /// Format string for output
    pub format: FormatTemplate,

    /// Unit that is used to display disk space. Options are B, KB, MB, GB and TB
    pub unit: String,

    /// Update interval in seconds
    #[serde(deserialize_with = "deserialize_duration")]
    pub interval: Duration,

    /// Diskspace warning (yellow)
    pub warning: f64,

    /// Diskspace alert (red)
    pub alert: f64,

    /// use absolute (unit) values for disk space alerts
    pub alert_absolute: bool,

    /// Alias that is displayed for path
    // DEPRECATED
    // TODO remove
    pub alias: String,
}

impl Default for DiskSpaceConfig {
    fn default() -> Self {
        Self {
            path: "/".to_string(),
            info_type: InfoType::Available,
            format: FormatTemplate::default(),
            unit: "GB".to_string(),
            interval: Duration::from_secs(20),
            warning: 20.,
            alert: 10.,
            alert_absolute: false,
            alias: "/".to_string(),
        }
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
            format: block_config.format.with_default("{available}")?,
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
                result = available as f64;
                alert_type = AlertType::Below;
            }
            InfoType::Free => {
                result = free as f64;
                alert_type = AlertType::Below;
            }
            InfoType::Used => {
                result = used as f64;
                alert_type = AlertType::Above;
            }
        }

        let percentage = result / (total as f64) * 100.;
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
        self.disk_space.set_texts(self.format.render(&values)?);

        // Send percentage to alert check if we don't want absolute alerts
        let alert_val = if self.alert_absolute {
            result
                / match self.unit {
                    Prefix::Tera => 1u64 << 40,
                    Prefix::Giga => 1u64 << 30,
                    Prefix::Mega => 1u64 << 20,
                    Prefix::Kilo => 1u64 << 10,
                    Prefix::One => 1u64,
                    _ => unreachable!(),
                } as f64
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

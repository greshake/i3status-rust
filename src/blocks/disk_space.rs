use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use crossbeam_channel::Sender;
use nix::sys::statvfs::statvfs;
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::Config;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::scheduler::Task;
use crate::util::{format_percent_bar, FormatTemplate};
use crate::widget::{I3BarWidget, State};
use crate::widgets::text::TextWidget;

#[derive(Copy, Clone, Debug, Deserialize, PartialEq, Eq)]
pub enum Unit {
    MB,
    GB,
    TB,
    TiB,
    GiB,
    MiB,
    Percent,
}

impl Unit {
    fn bytes_in_unit(unit: Unit, bytes: u64) -> f64 {
        match unit {
            Unit::MB => bytes as f64 / 1000. / 1000.,
            Unit::GB => bytes as f64 / 1000. / 1000. / 1000.,
            Unit::TB => bytes as f64 / 1000. / 1000. / 1000. / 1000.,
            Unit::MiB => bytes as f64 / 1024. / 1024.,
            Unit::GiB => bytes as f64 / 1024. / 1024. / 1024.,
            Unit::TiB => bytes as f64 / 1024. / 1024. / 1024. / 1024.,
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
    id: usize,
    disk_space: TextWidget,
    update_interval: Duration,
    alias: String,
    path: String,
    unit: Unit,
    info_type: InfoType,
    warning: f64,
    alert: f64,
    show_percentage: bool,
    show_bar: bool,
    alert_absolute: bool,
    format: FormatTemplate,
    icon: String,
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

    /// Currently supported options are available, free, total and used
    /// Sets value used for {percentage} calculation
    /// total is the same as used, use format to set format string for output
    #[serde(default = "DiskSpaceConfig::default_info_type")]
    pub info_type: InfoType,

    /// Format string for output
    /// placeholders: {percentage}, {bar}, {path}, {alias}, {available}, {free}, {total}, {used},
    ///               {unit}
    #[serde(default = "DiskSpaceConfig::default_format")]
    pub format: String,

    /// Unit that is used to display disk space. Options are MB, MiB, GB, GiB, TB and TiB
    #[serde(default = "DiskSpaceConfig::default_unit")]
    pub unit: Unit,

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

    /// Show percentage - deprecated for format string, kept for previous configs
    #[serde(default = "DiskSpaceConfig::default_show_percentage")]
    pub show_percentage: bool,

    /// Show percentage bar - deprecated for format string, kept for previous configs
    #[serde(default = "DiskSpaceConfig::default_show_bar")]
    pub show_bar: bool,

    /// use absolute (unit) values for disk space alerts
    #[serde(default = "DiskSpaceConfig::default_alert_absolute")]
    pub alert_absolute: bool,

    #[serde(default = "DiskSpaceConfig::default_color_overrides")]
    pub color_overrides: Option<BTreeMap<String, String>>,
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

    fn default_format() -> String {
        String::from("{alias} {available} {unit}")
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

    // Deprecated with format string, kept for previous config support
    fn default_show_percentage() -> bool {
        false
    }

    // Deprecated with format string, kept for previous config support
    fn default_show_bar() -> bool {
        false
    }

    fn default_alert_absolute() -> bool {
        false
    }

    fn default_color_overrides() -> Option<BTreeMap<String, String>> {
        None
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
        config: Config,
        _tx_update_request: Sender<Task>,
    ) -> Result<Self> {
        let icon = config.icons.get("disk_drive").cloned().unwrap_or_default();

        let disk_space = TextWidget::new(config, id);
        Ok(DiskSpace {
            id,
            update_interval: block_config.interval,
            disk_space,
            alias: block_config.alias,
            path: block_config.path,
            format: FormatTemplate::from_string(&block_config.format)?,
            info_type: block_config.info_type,
            unit: block_config.unit,
            warning: block_config.warning,
            alert: block_config.alert,
            show_percentage: block_config.show_percentage,
            show_bar: block_config.show_bar,
            alert_absolute: block_config.alert_absolute,
            icon,
        })
    }
}

impl Block for DiskSpace {
    fn update(&mut self) -> Result<Option<Update>> {
        let statvfs = statvfs(Path::new(self.path.as_str()))
            .block_error("disk_space", "failed to retrieve statvfs")?;

        let result;
        let total = (statvfs.blocks() as u64) * (statvfs.fragment_size() as u64);
        let used = ((statvfs.blocks() as u64) - (statvfs.blocks_free() as u64))
            * (statvfs.fragment_size() as u64);
        let available = (statvfs.blocks_available() as u64) * (statvfs.block_size() as u64);
        let free = (statvfs.blocks_free() as u64) * (statvfs.block_size() as u64);

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
            InfoType::Total => {
                // Deprecated: Same as Used - use format string to set output format
                // Kept for back-compatibility
                // Use format: "{used}/{total} {unit}" for previous format
                result = used;
                alert_type = AlertType::Above;
                self.format = FormatTemplate::from_string("{used}/{total} {unit}")?;
            }
            InfoType::Used => {
                result = used;
                alert_type = AlertType::Above;
            }
        }

        let percentage = (result as f32) / (total as f32) * 100f32;
        if self.show_percentage {
            self.format = FormatTemplate::from_string("{alias} {result} ({percentage}) {unit}")?;
        } else if self.show_bar {
            self.format = FormatTemplate::from_string("{alias} {result} {unit} {bar}")?;
        }

        let values = map!("{percentage}" => format!("{:.2}%", percentage),
        "{bar}" => format_percent_bar(percentage),
        "{alias}" => self.alias.clone(),
        "{unit}" => format!("{:?}", self.unit),
        "{path}" => self.path.clone(),
        "{total}" => format!("{:.2}", Unit::bytes_in_unit(self.unit, total)),
        "{used}" => format!("{:.2}", Unit::bytes_in_unit(self.unit, used)),
        "{available}" => format!("{:.2}", Unit::bytes_in_unit(self.unit, available)),
        "{free}" => format!("{:.2}", Unit::bytes_in_unit(self.unit, free)),
        "{icon}" => self.icon.to_string(),
        "{result}" => format!("{:.2}", result)
        );
        self.disk_space
            .set_text(self.format.render_static_str(&values)?);

        // Send percentage to alert check if we don't want absolute alerts
        let alert_val = if !self.alert_absolute {
            percentage as f64
        } else {
            Unit::bytes_in_unit(self.unit, result)
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

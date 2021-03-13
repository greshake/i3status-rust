use std::path::Path;
use std::time::Duration;

use crossbeam_channel::Sender;
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::SharedConfig;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::scheduler::Task;
use crate::util::read_file;
use crate::widgets::text::TextWidget;
use crate::widgets::I3BarWidget;

pub struct Uptime {
    id: usize,
    text: TextWidget,
    update_interval: Duration,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct UptimeConfig {
    /// Update interval in seconds
    #[serde(deserialize_with = "deserialize_duration")]
    pub interval: Duration,
}

impl Default for UptimeConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(60),
        }
    }
}

impl ConfigBlock for Uptime {
    type Config = UptimeConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        shared_config: SharedConfig,
        _tx_update_request: Sender<Task>,
    ) -> Result<Self> {
        Ok(Uptime {
            id,
            update_interval: block_config.interval,
            text: TextWidget::new(id, 0, shared_config).with_icon("uptime")?,
        })
    }
}

impl Block for Uptime {
    fn update(&mut self) -> Result<Option<Update>> {
        let uptime_raw = read_file("uptime", Path::new("/proc/uptime")).map_err(|e| {
            BlockError(
                "Uptime".to_owned(),
                format!("Uptime failed to read /proc/uptime: '{}'", e),
            )
        })?;
        let uptime = uptime_raw
            .split_whitespace()
            .next()
            .block_error("Uptime", "Uptime failed to read uptime string.")?;

        let total_seconds = uptime
            .parse::<f64>()
            .map(|x| x as u32)
            .block_error("Uptime", "Failed to convert uptime float to integer)")?;

        // split up seconds into more human readable portions
        let weeks = (total_seconds / 604_800) as u32;
        let rem_weeks = (total_seconds % 604_800) as u32;
        let days = (rem_weeks / 86_400) as u32;
        let rem_days = (rem_weeks % 86_400) as u32;
        let hours = (rem_days / 3600) as u32;
        let rem_hours = (rem_days % 3600) as u32;
        let minutes = (rem_hours / 60) as u32;
        let rem_minutes = (rem_hours % 60) as u32;
        let seconds = rem_minutes as u32;

        // Display the two largest units.
        let text = if hours == 0 && days == 0 && weeks == 0 {
            format!("{}m {}s", minutes, seconds)
        } else if hours > 0 && days == 0 && weeks == 0 {
            format!("{}h {}m", hours, minutes)
        } else if days > 0 && weeks == 0 {
            format!("{}d {}h", days, hours)
        } else if days == 0 && weeks > 0 {
            format!("{}w {}h", weeks, hours)
        } else if weeks > 0 {
            format!("{}w {}d", weeks, days)
        } else {
            unreachable!()
        };
        self.text.set_text(text);
        Ok(Some(self.update_interval.into()))
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.text]
    }

    fn id(&self) -> usize {
        self.id
    }
}

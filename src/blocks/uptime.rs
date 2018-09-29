use std::path::Path;
use std::time::Duration;

use chan::Sender;
use uuid::Uuid;

use block::{Block, ConfigBlock};
use config::Config;
use de::deserialize_duration;
use errors::*;
use scheduler::Task;
use util::read_file;
use widgets::text::TextWidget;
use widget::I3BarWidget;

pub struct Uptime {
    text: TextWidget,
    id: String,
    update_interval: Duration,

    //useful, but optional
    #[allow(dead_code)] config: Config,
    #[allow(dead_code)] tx_update_request: Sender<Task>,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct UptimeConfig {
    /// Update interval in seconds
    #[serde(default = "UptimeConfig::default_interval", deserialize_with = "deserialize_duration")]
    pub interval: Duration,
}

impl UptimeConfig {
    fn default_interval() -> Duration {
        Duration::from_secs(60)
    }
}

impl ConfigBlock for Uptime {
    type Config = UptimeConfig;

    fn new(block_config: Self::Config, config: Config, tx_update_request: Sender<Task>) -> Result<Self> {
        Ok(Uptime {
            id: format!("{}", Uuid::new_v4().to_simple()),
            update_interval: block_config.interval,
            text: TextWidget::new(config.clone()).with_icon("uptime"),
            tx_update_request: tx_update_request,
            config: config,
        })
    }
}

impl Block for Uptime {
    fn update(&mut self) -> Result<Option<Duration>> {
        let uptime_raw = match read_file("uptime", Path::new("/proc/uptime")) {
            Ok(file) => file,
            Err(e) => {
                return Err(BlockError(
                    "Uptime".to_owned(),
                    format!("Uptime failed to read /proc/uptime: '{}'", e),
                ));
            }
        };
        let uptime = match uptime_raw.split_whitespace().nth(0) {
            Some(uptime) => uptime,
            None => {
                return Err(BlockError(
                    "Uptime".to_owned(),
                    "Uptime failed to read uptime string.".to_owned(),
                ));
            }
        };

        let total_seconds = match uptime.parse::<f64>() {
            Ok(uptime) => uptime as u32,
            Err(e) => {
                return Err(BlockError(
                    "Uptime".to_owned(),
                    format!("Uptime failed to convert uptime float to integer: '{}')", e),
                ));
            }
        };

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
        Ok(Some(self.update_interval))
    }

    fn view(&self) -> Vec<&I3BarWidget> {
        vec![&self.text]
    }

    fn id(&self) -> &str {
        &self.id
    }
}

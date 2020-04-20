use crossbeam_channel::Sender;
use serde_derive::Deserialize;
use std::time::Duration;

use crate::blocks::{Block, ConfigBlock};
use crate::config::Config;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::input::I3BarEvent;
use crate::scheduler::Task;
use crate::widget::I3BarWidget;
use crate::widgets::text::TextWidget;

use uuid::Uuid;

pub struct Taskwarrior {
    text: TextWidget,
    id: String,
    update_interval: Duration,

    //useful, but optional
    #[allow(dead_code)]
    config: Config,
    #[allow(dead_code)]
    tx_update_request: Sender<Task>,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct TaskwarriorConfig {
    /// Update interval in seconds
    #[serde(
        default = "TaskwarriorConfig::default_interval",
        deserialize_with = "deserialize_duration"
    )]
    pub interval: Duration,
}

impl TaskwarriorConfig {
    fn default_interval() -> Duration {
        Duration::from_secs(5)
    }
}

impl ConfigBlock for Taskwarrior {
    type Config = TaskwarriorConfig;

    fn new(
        block_config: Self::Config,
        config: Config,
        tx_update_request: Sender<Task>,
    ) -> Result<Self> {
        Ok(Taskwarrior {
            id: Uuid::new_v4().to_simple().to_string(),
            update_interval: block_config.interval,
            text: TextWidget::new(config.clone()).with_text("Taskwarrior"),
            tx_update_request,
            config,
        })
    }
}

impl Block for Taskwarrior {
    fn update(&mut self) -> Result<Option<Duration>> {
        Ok(Some(self.update_interval))
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.text]
    }

    fn click(&mut self, _: &I3BarEvent) -> Result<()> {
        Ok(())
    }

    fn id(&self) -> &str {
        &self.id
    }
}

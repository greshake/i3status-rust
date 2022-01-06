use std::time::Duration;

use crossbeam_channel::Sender;
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::SharedConfig;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::protocol::i3bar_event::I3BarEvent;
use crate::scheduler::Task;
use crate::widgets::text::TextWidget;
use crate::widgets::I3BarWidget;

pub struct Template {
    id: usize,
    text: TextWidget,
    update_interval: Duration,

    //useful, but optional
    #[allow(dead_code)]
    shared_config: SharedConfig,
    #[allow(dead_code)]
    tx_update_request: Sender<Task>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct TemplateConfig {
    /// Update interval in seconds
    #[serde(deserialize_with = "deserialize_duration")]
    pub interval: Duration,
}

impl Default for TemplateConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(5),
        }
    }
}

impl ConfigBlock for Template {
    type Config = TemplateConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        shared_config: SharedConfig,
        tx_update_request: Sender<Task>,
    ) -> Result<Self> {
        let text = TextWidget::new(id, 0, shared_config.clone()).with_text("Template");

        Ok(Template {
            id,
            update_interval: block_config.interval,
            text,
            tx_update_request,
            shared_config,
        })
    }
}

impl Block for Template {
    fn update(&mut self) -> Result<Option<Update>> {
        Ok(Some(self.update_interval.into()))
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.text]
    }

    fn click(&mut self, _: &I3BarEvent) -> Result<()> {
        Ok(())
    }

    fn id(&self) -> usize {
        self.id
    }
}

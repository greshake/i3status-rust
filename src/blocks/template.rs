use std::collections::BTreeMap;
use std::time::Duration;

use crossbeam_channel::Sender;
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::Config;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::input::I3BarEvent;
use crate::scheduler::Task;
use crate::widget::I3BarWidget;
use crate::widgets::text::TextWidget;

pub struct Template {
    id: usize,
    text: TextWidget,
    update_interval: Duration,

    //useful, but optional
    #[allow(dead_code)]
    config: Config,
    #[allow(dead_code)]
    tx_update_request: Sender<Task>,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct TemplateConfig {
    /// Update interval in seconds
    #[serde(
        default = "TemplateConfig::default_interval",
        deserialize_with = "deserialize_duration"
    )]
    pub interval: Duration,

    #[serde(default = "TemplateConfig::default_color_overrides")]
    pub color_overrides: Option<BTreeMap<String, String>>,
}

impl TemplateConfig {
    fn default_interval() -> Duration {
        Duration::from_secs(5)
    }

    fn default_color_overrides() -> Option<BTreeMap<String, String>> {
        None
    }
}

impl ConfigBlock for Template {
    type Config = TemplateConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        config: Config,
        tx_update_request: Sender<Task>,
    ) -> Result<Self> {
        let text = TextWidget::new(config.clone(), id).with_text("Template");

        Ok(Template {
            id,
            update_interval: block_config.interval,
            text,
            tx_update_request,
            config,
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

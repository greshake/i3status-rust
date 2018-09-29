use std::time::Duration;
use chan::Sender;

use block::{Block, ConfigBlock};
use config::Config;
use de::deserialize_duration;
use errors::*;
use widgets::text::TextWidget;
use widget::I3BarWidget;
use input::I3BarEvent;
use scheduler::Task;

use uuid::Uuid;

pub struct Template {
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
pub struct TemplateConfig {
    /// Update interval in seconds
    #[serde(default = "TemplateConfig::default_interval", deserialize_with = "deserialize_duration")]
    pub interval: Duration,
}

impl TemplateConfig {
    fn default_interval() -> Duration {
        Duration::from_secs(5)
    }
}

impl ConfigBlock for Template {
    type Config = TemplateConfig;

    fn new(block_config: Self::Config, config: Config, tx_update_request: Sender<Task>) -> Result<Self> {
        Ok(Template {
            id: format!("{}", Uuid::new_v4().to_simple()),
            update_interval: block_config.interval,
            text: TextWidget::new(config.clone()).with_text("Template"),
            tx_update_request: tx_update_request,
            config: config,
        })
    }
}

impl Block for Template {
    fn update(&mut self) -> Result<Option<Duration>> {
        Ok(Some(self.update_interval))
    }

    fn view(&self) -> Vec<&I3BarWidget> {
        vec![&self.text]
    }

    fn click(&mut self, _: &I3BarEvent) -> Result<()> {
        Ok(())
    }

    fn id(&self) -> &str {
        &self.id
    }
}

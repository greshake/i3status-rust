use std::time::Duration;
use chan::Sender;

use block::{Block, ConfigBlock};
use config::Config;
use de::deserialize_duration;
use errors::*;
use widgets::text::TextWidget;
use widget::{I3BarWidget, State};
use input::I3BarEvent;
use scheduler::Task;
use maildir::Maildir as ExtMaildir;

use uuid::Uuid;

pub struct Maildir {
    text: TextWidget,
    id: String,
    update_interval: Duration,
    inboxes: Vec<String>,
    threshold_warning: usize,
    threshold_critical: usize,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct MaildirConfig {
    /// Update interval in seconds
    #[serde(default = "MaildirConfig::default_interval", deserialize_with = "deserialize_duration")]
    pub interval: Duration,
    pub inboxes: Vec<String>,
    #[serde(default = "MaildirConfig::default_threshold_warning")]
    pub threshold_warning: usize,
    #[serde(default = "MaildirConfig::default_threshold_critical")]
    pub threshold_critical: usize,
}

impl MaildirConfig {
    fn default_interval() -> Duration {
        Duration::from_secs(5)
    }
    fn default_threshold_warning() -> usize {
        1 as usize
    }
    fn default_threshold_critical() -> usize {
        10 as usize
    }
}

impl ConfigBlock for Maildir {
    type Config = MaildirConfig;

    fn new(block_config: Self::Config, config: Config, _tx_update_request: Sender<Task>) -> Result<Self> {
        Ok(Maildir {
            id: format!("{}", Uuid::new_v4().to_simple()),
            update_interval: block_config.interval,
            text: TextWidget::new(config.clone())
                .with_icon("mail")
                .with_text(""),
            inboxes: block_config.inboxes,
            threshold_warning: block_config.threshold_warning,
            threshold_critical: block_config.threshold_critical,
        })
    }
}

impl Block for Maildir {
    fn update(&mut self) -> Result<Option<Duration>> {
        let mut newmails = 0;
        for inbox in &self.inboxes {
            let isl: &str = &inbox[..];
            let maildir = ExtMaildir::from(isl);
            newmails += maildir.count_new();
        }
        let mut state = { State::Idle };
        if newmails >= self.threshold_critical {
            state = { State::Critical };
        } else if newmails >= self.threshold_warning {
            state = { State::Warning };
        }
        self.text.set_state(state);
        self.text.set_text(format!("{}", newmails));
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

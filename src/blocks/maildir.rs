use crossbeam_channel::Sender;
use serde_derive::Deserialize;
use std::time::Duration;

use crate::blocks::{Block, ConfigBlock};
use crate::config::Config;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::input::I3BarEvent;
use crate::scheduler::Task;
use crate::widget::{I3BarWidget, State};
use crate::widgets::text::TextWidget;
use maildir::Maildir as ExtMaildir;

use uuid::Uuid;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MailType {
    New,
    Cur,
    All,
}

impl MailType {
    fn count_mail(&self, maildir: &ExtMaildir) -> usize {
        match self {
            MailType::New => maildir.count_new(),
            MailType::Cur => maildir.count_cur(),
            MailType::All => maildir.count_new() + maildir.count_cur(),
        }
    }
}

impl Default for MailType {
    fn default() -> MailType {
        MailType::New
    }
}

pub struct Maildir {
    text: TextWidget,
    id: String,
    update_interval: Duration,
    inboxes: Vec<String>,
    threshold_warning: usize,
    threshold_critical: usize,
    display_type: MailType,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct MaildirConfig {
    /// Update interval in seconds
    #[serde(
        default = "MaildirConfig::default_interval",
        deserialize_with = "deserialize_duration"
    )]
    pub interval: Duration,
    pub inboxes: Vec<String>,
    #[serde(default = "MaildirConfig::default_threshold_warning")]
    pub threshold_warning: usize,
    #[serde(default = "MaildirConfig::default_threshold_critical")]
    pub threshold_critical: usize,
    #[serde(default)]
    pub display_type: MailType,
    #[serde(default = "MaildirConfig::default_icon")]
    pub icon: bool,
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
    fn default_icon() -> bool {
        true
    }
}

impl ConfigBlock for Maildir {
    type Config = MaildirConfig;

    fn new(
        block_config: Self::Config,
        config: Config,
        _tx_update_request: Sender<Task>,
    ) -> Result<Self> {
        let widget = TextWidget::new(config.clone()).with_text("");
        Ok(Maildir {
            id: Uuid::new_v4().to_simple().to_string(),
            update_interval: block_config.interval,
            text: if block_config.icon {
                widget.with_icon("mail")
            } else {
                widget
            },
            inboxes: block_config.inboxes,
            threshold_warning: block_config.threshold_warning,
            threshold_critical: block_config.threshold_critical,
            display_type: block_config.display_type,
        })
    }
}

impl Block for Maildir {
    fn update(&mut self) -> Result<Option<Duration>> {
        let mut newmails = 0;
        for inbox in &self.inboxes {
            let isl: &str = &inbox[..];
            let maildir = ExtMaildir::from(isl);
            newmails += self.display_type.count_mail(&maildir)
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

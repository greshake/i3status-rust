use std::time::Duration;

use crossbeam_channel::Sender;
use maildir::Maildir as ExtMaildir;
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::SharedConfig;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::scheduler::Task;
use crate::widgets::text::TextWidget;
use crate::widgets::{I3BarWidget, State};

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

pub struct Maildir {
    id: usize,
    text: TextWidget,
    update_interval: Duration,
    inboxes: Vec<String>,
    threshold_warning: usize,
    threshold_critical: usize,
    display_type: MailType,
}

//TODO add `format`
#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct MaildirConfig {
    /// Update interval in seconds
    #[serde(deserialize_with = "deserialize_duration")]
    pub interval: Duration,
    pub inboxes: Vec<String>,
    pub threshold_warning: usize,
    pub threshold_critical: usize,
    pub display_type: MailType,
    // DEPRECATED
    pub icon: bool,
}

impl Default for MaildirConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(5),
            inboxes: Vec::new(),
            threshold_warning: 1,
            threshold_critical: 10,
            display_type: MailType::New,
            icon: true,
        }
    }
}

impl ConfigBlock for Maildir {
    type Config = MaildirConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        shared_config: SharedConfig,
        _tx_update_request: Sender<Task>,
    ) -> Result<Self> {
        let widget = TextWidget::new(id, 0, shared_config).with_text("");
        Ok(Maildir {
            id,
            update_interval: block_config.interval,
            text: if block_config.icon {
                widget.with_icon("mail")?
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
    fn update(&mut self) -> Result<Option<Update>> {
        let mut newmails = 0;
        for inbox in &self.inboxes {
            let isl: &str = &inbox[..];
            let maildir = ExtMaildir::from(isl);
            newmails += self.display_type.count_mail(&maildir)
        }
        let mut state = State::Idle;
        if newmails >= self.threshold_critical {
            state = State::Critical;
        } else if newmails >= self.threshold_warning {
            state = State::Warning;
        }
        self.text.set_state(state);
        self.text.set_text(format!("{}", newmails));
        Ok(Some(self.update_interval.into()))
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.text]
    }

    fn id(&self) -> usize {
        self.id
    }
}

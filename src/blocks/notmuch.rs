use std::env;
use std::time::Duration;

use crossbeam_channel::Sender;
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::SharedConfig;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::protocol::i3bar_event::{I3BarEvent, MouseButton};
use crate::scheduler::Task;
use crate::widgets::text::TextWidget;
use crate::widgets::{I3BarWidget, State};

pub struct Notmuch {
    id: usize,
    text: TextWidget,
    update_interval: Duration,
    query: String,
    db: String,
    threshold_info: u32,
    threshold_good: u32,
    threshold_warning: u32,
    threshold_critical: u32,
    name: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct NotmuchConfig {
    /// Update interval in seconds
    #[serde(deserialize_with = "deserialize_duration")]
    pub interval: Duration,
    pub maildir: String,
    pub query: String,
    pub threshold_warning: u32,
    pub threshold_critical: u32,
    pub threshold_info: u32,
    pub threshold_good: u32,
    pub name: Option<String>,
    // DEPRECATED
    pub no_icon: bool,
}

impl Default for NotmuchConfig {
    fn default() -> Self {
        #[allow(deprecated)]
        let home_dir = match env::home_dir() {
            Some(path) => path.into_os_string().into_string().unwrap(),
            None => "".to_owned(),
        };
        let maildir = format!("{}/.mail", home_dir);

        Self {
            interval: Duration::from_secs(10),
            maildir,
            query: "".to_string(),
            threshold_warning: std::u32::MAX,
            threshold_critical: std::u32::MAX,
            threshold_info: std::u32::MAX,
            threshold_good: std::u32::MAX,
            name: None,
            no_icon: false,
        }
    }
}

fn run_query(db_path: &str, query_string: &str) -> std::result::Result<u32, notmuch::Error> {
    let db = notmuch::Database::open(&db_path, notmuch::DatabaseMode::ReadOnly)?;
    let query = db.create_query(query_string)?;
    query.count_messages()
}

impl ConfigBlock for Notmuch {
    type Config = NotmuchConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        shared_config: SharedConfig,
        _tx_update_request: Sender<Task>,
    ) -> Result<Self> {
        let mut widget = TextWidget::new(id, 0, shared_config);
        if !block_config.no_icon {
            widget.set_icon("mail")?;
        }
        Ok(Notmuch {
            id,
            update_interval: block_config.interval,
            db: block_config.maildir,
            query: block_config.query,
            threshold_info: block_config.threshold_info,
            threshold_good: block_config.threshold_good,
            threshold_warning: block_config.threshold_warning,
            threshold_critical: block_config.threshold_critical,
            name: block_config.name,
            text: widget,
        })
    }
}

impl Notmuch {
    fn update_text(&mut self, count: u32) {
        let text = match self.name {
            Some(ref s) => format!("{}:{}", s, count),
            _ => format!("{}", count),
        };
        self.text.set_text(text);
    }

    fn update_state(&mut self, count: u32) {
        let mut state = State::Idle;
        if count >= self.threshold_critical {
            state = State::Critical;
        } else if count >= self.threshold_warning {
            state = State::Warning;
        } else if count >= self.threshold_good {
            state = State::Good;
        } else if count >= self.threshold_info {
            state = State::Info;
        }
        self.text.set_state(state);
    }
}

impl Block for Notmuch {
    fn update(&mut self) -> Result<Option<Update>> {
        match run_query(&self.db, &self.query) {
            Ok(count) => {
                self.update_text(count);
                self.update_state(count);
                Ok(Some(self.update_interval.into()))
            }
            Err(e) => Err(BlockError("notmuch".to_string(), e.to_string())),
        }
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.text]
    }

    fn click(&mut self, event: &I3BarEvent) -> Result<()> {
        if event.button == MouseButton::Left {
            self.update()?;
        }

        Ok(())
    }

    fn id(&self) -> usize {
        self.id
    }
}

use std::collections::BTreeMap;
use std::env;
use std::time::Duration;

use crossbeam_channel::Sender;
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::Config;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::input::{I3BarEvent, MouseButton};
use crate::scheduler::Task;
use crate::widget::{I3BarWidget, State};
use crate::widgets::text::TextWidget;

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

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct NotmuchConfig {
    /// Update interval in seconds
    #[serde(
        default = "NotmuchConfig::default_interval",
        deserialize_with = "deserialize_duration"
    )]
    pub interval: Duration,
    #[serde(default = "NotmuchConfig::default_maildir")]
    pub maildir: String,
    #[serde(default = "NotmuchConfig::default_query")]
    pub query: String,
    #[serde(default = "NotmuchConfig::default_threshold_warning")]
    pub threshold_warning: u32,
    #[serde(default = "NotmuchConfig::default_threshold_critical")]
    pub threshold_critical: u32,
    #[serde(default = "NotmuchConfig::default_threshold_info")]
    pub threshold_info: u32,
    #[serde(default = "NotmuchConfig::default_threshold_good")]
    pub threshold_good: u32,
    #[serde(default = "NotmuchConfig::default_name")]
    pub name: Option<String>,
    #[serde(default = "NotmuchConfig::default_no_icon")]
    pub no_icon: bool,

    #[serde(default = "NotmuchConfig::default_color_overrides")]
    pub color_overrides: Option<BTreeMap<String, String>>,
}

impl NotmuchConfig {
    fn default_interval() -> Duration {
        Duration::from_secs(10)
    }

    fn default_maildir() -> String {
        #[allow(deprecated)]
        let home_dir = match env::home_dir() {
            Some(path) => path.into_os_string().into_string().unwrap(),
            None => "".to_owned(),
        };

        format!("{}/.mail", home_dir)
    }

    fn default_query() -> String {
        "".to_owned()
    }

    fn default_threshold_info() -> u32 {
        <u32>::max_value()
    }

    fn default_threshold_good() -> u32 {
        <u32>::max_value()
    }

    fn default_threshold_warning() -> u32 {
        <u32>::max_value()
    }

    fn default_threshold_critical() -> u32 {
        <u32>::max_value()
    }

    fn default_name() -> Option<String> {
        None
    }

    fn default_no_icon() -> bool {
        false
    }

    fn default_color_overrides() -> Option<BTreeMap<String, String>> {
        None
    }
}

fn run_query(db_path: &str, query_string: &str) -> std::result::Result<u32, notmuch::Error> {
    let db = notmuch::Database::open(&db_path, notmuch::DatabaseMode::ReadOnly)?;
    let query = db.create_query(query_string)?;
    Ok(query.count_messages()?)
}

impl ConfigBlock for Notmuch {
    type Config = NotmuchConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        config: Config,
        _tx_update_request: Sender<Task>,
    ) -> Result<Self> {
        let mut widget = TextWidget::new(config, id);
        if !block_config.no_icon {
            widget.set_icon("mail");
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
        if event.matches_id(self.id) && event.button == MouseButton::Left {
            self.update()?;
        }

        Ok(())
    }

    fn id(&self) -> usize {
        self.id
    }
}

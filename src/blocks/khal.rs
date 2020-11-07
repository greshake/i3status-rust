use std::process::Command;
use std::time::Duration;

use crossbeam_channel::Sender;
use serde_derive::Deserialize;
use uuid::Uuid;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::Config;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::input::I3BarEvent;
use crate::scheduler::Task;
use crate::widget::{I3BarWidget, State};
use crate::widgets::text::TextWidget;

pub struct Khal {
    text: TextWidget,
    id: String,
    update_interval: Duration,
    //calendars: Vec<String>,
    threshold_warning: usize,
    threshold_critical: usize,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct KhalConfig {
    /// Update interval in seconds
    #[serde(
        default = "KhalConfig::default_interval",
        deserialize_with = "deserialize_duration"
    )]
    pub interval: Duration,
    // pub calendars: Vec<String>,
    #[serde(default = "KhalConfig::default_threshold_warning")]
    pub threshold_warning: usize,
    #[serde(default = "KhalConfig::default_threshold_critical")]
    pub threshold_critical: usize,
    #[serde(default = "KhalConfig::default_icon")]
    pub icon: bool,
}

impl KhalConfig {
    fn default_interval() -> Duration {
        Duration::from_secs(60)
    }
    fn default_threshold_warning() -> usize {
        1 as usize
    }
    fn default_threshold_critical() -> usize {
        5 as usize
    }
    fn default_icon() -> bool {
        true
    }
}

impl ConfigBlock for Khal {
    type Config = KhalConfig;

    fn new(
        block_config: Self::Config,
        config: Config,
        _tx_update_request: Sender<Task>,
    ) -> Result<Self> {
        let widget = TextWidget::new(config).with_text("");
        Ok(Khal {
            id: Uuid::new_v4().to_simple().to_string(),
            update_interval: block_config.interval,
            text: if block_config.icon {
                widget.with_icon("calendar")
            } else {
                widget
            },
            // calendars: block_config.calendars,
            threshold_warning: block_config.threshold_warning,
            threshold_critical: block_config.threshold_critical,
        })
    }
}

impl Block for Khal {
    fn update(&mut self) -> Result<Option<Update>> {
        let mut events = 0;

        let khal_cmd = Command::new("khal")
            .arg("list")
            .arg("-df")
            .arg("{name}")
            .arg("--notstarted")
            .arg("today")
            .arg("today")
            .output()
            .expect("failed to execute process");

        let khal_output = String::from_utf8(khal_cmd.stdout).unwrap();
        let mut lines = khal_output.lines();
        let dayline = match lines.nth(0) {
            Some(dl) => dl,
            None => "None",
        };

        if dayline.trim() == "Today" {
            events = lines.count()
        }

        let mut state = State::Idle;
        if events >= self.threshold_critical {
            state = State::Critical;
        } else if events >= self.threshold_warning {
            state = State::Warning;
        }
        self.text.set_state(state);
        self.text.set_text(format!("{}", events));
        Ok(Some(self.update_interval.into()))
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

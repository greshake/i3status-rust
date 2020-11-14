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
use chrono::{Local, NaiveTime};

pub struct Khal {
    text: TextWidget,
    id: String,
    update_interval: Duration,
    threshold_warning: i64,
    threshold_critical: i64,
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
    pub threshold_warning: i64,
    #[serde(default = "KhalConfig::default_threshold_critical")]
    pub threshold_critical: i64,
    #[serde(default = "KhalConfig::default_icon")]
    pub icon: bool,
}

impl KhalConfig {
    fn default_interval() -> Duration {
        Duration::from_secs(60)
    }
    fn default_threshold_warning() -> i64 {
        60 as i64
    }
    fn default_threshold_critical() -> i64 {
        15 as i64
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
            threshold_warning: block_config.threshold_warning,
            threshold_critical: block_config.threshold_critical,
        })
    }
}

impl Block for Khal {
    fn update(&mut self) -> Result<Option<Update>> {
        let mut events: Vec<String> = Vec::new();
        let now = Local::now().time();
        let from = now.format("%H:%M").to_string();

        let khal_cmd = Command::new("khal")
            .arg("list")
            .arg("-df")
            .arg("{name}")
            .arg("-f")
            .arg("{start-time}")
            .arg("--notstarted")
            .arg(&from)
            .output();

        let khal_output = khal_cmd.block_error("khal", "failed to run command")?;
        let khal_stdout =
            String::from_utf8(khal_output.stdout)
            .block_error("khal", "can't read output")?;

        let mut lines = khal_stdout.lines();
        let dayline = lines.nth(0).block_error("khal", "output seems empty");

        if dayline.unwrap().trim() == "Today" {
            for e in lines {
                events.push(e.to_string())
            }
        }

        // get duration up to next event and set state
        let mut state = State::Idle;

        let mut event_remaining: i64 = 24 * 60;
        let event_count = events.len();
        for e in events.iter() {
            let e_start = match NaiveTime::parse_from_str(e, "%H:%M") {
                Ok(s) => s,
                Err(_f) => NaiveTime::from_hms(0, 0, 0),
            };
            let diff = e_start - now;
            if (diff.num_minutes() < event_remaining) && (diff.num_minutes() >= 0) {
                event_remaining = diff.num_minutes()
            }

            if event_remaining >= 0 {
                if event_remaining <= self.threshold_warning {
                    state = State::Warning;
                }
                if event_remaining <= self.threshold_critical {
                    state = State::Critical;
                }
            }
        }

        self.text.set_state(state);
        self.text.set_text(format!("{}", event_count));
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

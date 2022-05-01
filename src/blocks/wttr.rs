use std::time::Duration;

use crossbeam_channel::Sender;
use curl::easy::{Easy, List};
use regex::Regex;
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::SharedConfig;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::protocol::i3bar_event::I3BarEvent;
use crate::scheduler::Task;
use crate::widgets::text::TextWidget;
use crate::widgets::I3BarWidget;

pub struct Wttr {
    id: usize,
    text: TextWidget,
    update_interval: Duration,
    #[allow(dead_code)]
    shared_config: SharedConfig,
    query: String,
    location: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct WttrConfig {
    #[serde(deserialize_with = "deserialize_duration")]
    interval: Duration,
    query: String,
    location: Option<String>,
}

impl Default for WttrConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(900),
            query: String::from("format=1"),
            location: None,
        }
    }
}

impl ConfigBlock for Wttr {
    type Config = WttrConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        shared_config: SharedConfig,
        _tx_update_request: Sender<Task>,
    ) -> Result<Self> {
        let text = TextWidget::new(id, 0, shared_config.clone()).with_text("Wttr");

        Ok(Wttr {
            id,
            update_interval: block_config.interval,
            text,
            query: block_config.query,
            location: block_config.location,
            shared_config,
        })
    }
}

const BASE_URL: &str = "https://wttr.in/";

impl Block for Wttr {
    fn update(&mut self) -> Result<Option<Update>> {
        let url = match self.location {
            Some(ref location) => format!("{}{}?{}", BASE_URL, location, self.query),
            _ => format!("{}?{}", BASE_URL, self.query),
        };

        let mut data = Vec::new();
        let mut handle = Easy::new();
        let mut list = List::new();
        list.append("User-Agent: curl/7.81.0").unwrap();
        handle.http_headers(list).unwrap();
        handle.url(&url).unwrap();
        {
            let mut transfer = handle.transfer();
            transfer
                .write_function(|new_data| {
                    data.extend_from_slice(new_data);
                    Ok(new_data.len())
                })
                .unwrap();
            transfer.perform().unwrap();
        }

        let body = String::from_utf8(data).expect("body is not valid UTF8!");

        // Pre-defined formats add a newline ('\n') to the end. Get rid of it.
        let trimmed = body.trim();

        // I don't know why but there are 3 whitespaces added to condition.
        // This looks odd so get rid of it.
        let rg = Regex::new(r"  +").unwrap();
        let cleansed = rg.replace_all(trimmed, " ");

        self.text.set_text(cleansed.to_string());
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

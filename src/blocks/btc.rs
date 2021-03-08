use std::time::Duration;

use crossbeam_channel::Sender;
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::SharedConfig;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::input::I3BarEvent;
use crate::scheduler::Task;
use crate::widgets::text::TextWidget;
use crate::widgets::I3BarWidget;

pub struct Bitcoin {
    id: usize,
    text: TextWidget,
    update_interval: Duration,

    //useful, but optional
    #[allow(dead_code)]
    shared_config: SharedConfig,
    #[allow(dead_code)]
    tx_update_request: Sender<Task>,

    // for some currency
    currency: String,

}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct BitcoinConfig {
    /// Update interval in seconds
    #[serde(
        default = "BitcoinConfig::default_interval",
        deserialize_with = "deserialize_duration"
    )]
    pub interval: Duration,
    #[serde(default = "BitcoinConfig::default_currency")]
    pub currency: String,
}

impl BitcoinConfig {
    fn default_interval() -> Duration {
        Duration::from_secs(60)
    }

    fn default_currency() -> String {
        String::from("USD")
    }
}

impl ConfigBlock for Bitcoin {
    type Config = BitcoinConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        shared_config: SharedConfig,
        tx_update_request: Sender<Task>,
    ) -> Result<Self> {
        let text = TextWidget::new(id, 0, shared_config.clone()).with_text("Bitcoin");

        Ok(Bitcoin {
            id,
            update_interval: block_config.interval,
            text,
            tx_update_request,
            shared_config,
            
            currency: block_config.currency,
        })
    }
}

impl Block for Bitcoin {
    fn update(&mut self) -> Result<Option<Update>> {
        let mut handle = curl::easy::Easy::new();
        let mut buf = Vec::new();

        handle.url("https://blockchain.info/ticker")?;
        {
            let mut handle = handle.transfer();
            handle.write_function(|data| {
                    buf.extend_from_slice(data);
                    Ok(data.len())
                }
            )?;
            handle.perform()?;
        }

        let string = String::from_utf8(buf).unwrap_or("None".to_string());
        let mut parsed = BTCParser::default();
        for mut each in string.lines() { 
            each = each.trim_start();
            each = each.trim_end();
            if each[1..].starts_with(&format!("{}", self.currency)) {
                parsed = BTCParser::new(&each[8..each.len() - 1]);
            }
            if each.len() > 1 {
                //println!("{}", &each[8..each.len() - 1])
            }
        }

        let text = TextWidget::new(self.id, 0, self.shared_config.clone());
        self.text = text.with_text(&format!("1 BTC to {}: {} {}", self.currency, parsed.last, parsed.symbol)); 

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

#[derive(serde_derive::Deserialize, Clone, Debug)]
struct BTCParser {
    #[serde(rename(deserialize = "15m"))]
    m15: f32,
    last: f32,
    buy: f32,
    sell: f32,
    symbol: String,
}

impl BTCParser {
    fn new(json: &str) -> Self {
        let s = serde_json::from_str(json).unwrap_or(BTCParser::default());
        return s
    }
}

impl Default for BTCParser {
    fn default() -> Self {
        Self {
            m15: 0f32,
            last: 0f32,
            buy: 0f32,
            sell: 0f32,
            symbol: "None".to_string()
        }
        
    } 
}

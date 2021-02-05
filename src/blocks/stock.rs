use std::collections::BTreeMap;
use std::time::Duration;

use crossbeam_channel::Sender;
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::Config;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::input::I3BarEvent;
use crate::scheduler::Task;
use crate::util::pseudo_uuid;
use crate::widget::I3BarWidget;
use crate::widgets::text::TextWidget;

use yahoo_finance_api as yahoo;

impl From<yahoo::YahooError> for Error {
    fn from(error: yahoo::YahooError) -> Self {
        BlockError("Stock block".to_string(), format!("{}", error))
    }
}

pub struct Stock {
    text: TextWidget,
    id: String,
    update_interval: Duration,

    //useful, but optional
    #[allow(dead_code)]
    config: Config,
    #[allow(dead_code)]
    tx_update_request: Sender<Task>,
    symbol: String,
    provider: yahoo::YahooConnector,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct StockConfig {
    /// Update interval in seconds
    #[serde(
        default = "StockConfig::default_interval",
        deserialize_with = "deserialize_duration"
    )]
    pub interval: Duration,

    #[serde(default = "StockConfig::default_color_overrides")]
    pub color_overrides: Option<BTreeMap<String, String>>,

    pub symbol: String,
}

impl StockConfig {
    fn default_interval() -> Duration {
        Duration::from_secs(60)
    }

    fn default_color_overrides() -> Option<BTreeMap<String, String>> {
        None
    }
}

impl ConfigBlock for Stock {
    type Config = StockConfig;

    fn new(
        block_config: Self::Config,
        config: Config,
        tx_update_request: Sender<Task>,
    ) -> Result<Self> {
        let id = pseudo_uuid();
        let text = TextWidget::new(config.clone(), &id).with_text("Stock");
        let symbol = block_config.symbol;
        let provider = yahoo::YahooConnector::new();
        let update_interval = if block_config.interval.as_secs() < 60 {
            StockConfig::default_interval()
        } else {
            block_config.interval
        };

        Ok(Stock {
            id,
            text,
            tx_update_request,
            update_interval,
            symbol,
            provider,
            config,
        })
    }
}

impl Block for Stock {
    fn update(&mut self) -> Result<Option<Update>> {
        let updates = format!("{}m", self.update_interval.as_secs() / 60);
        let response = self.provider.get_latest_quotes(&self.symbol, &updates)?;
        let quote = response.last_quote()?;
        let txt = format!("{}: {:.2}", self.symbol, quote.close);
        self.text.set_text(txt);
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

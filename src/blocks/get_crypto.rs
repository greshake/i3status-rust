use std::time::Duration;

use crossbeam_channel::Sender;
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::SharedConfig;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::formatting::value::Value;
use crate::formatting::FormatTemplate;
use crate::http;
use crate::protocol::i3bar_event::I3BarEvent;
use crate::scheduler::Task;
use crate::widgets::text::TextWidget;
use crate::widgets::I3BarWidget;

pub struct GetCrypto {
    id: usize,
    output: TextWidget,
    format: FormatTemplate,
    update_interval: Duration,
    reference_currency: String,
    cryptocurrency: String,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct GetCryptoConfig {
    /// Update interval in seconds
    #[serde(deserialize_with = "deserialize_duration")]
    pub interval: Duration,

    /// Format override
    pub format: FormatTemplate,

    /// Reference currency
    pub reference_currency: String,

    /// Crypto currency to show
    pub cryptocurrency: String,
}

impl Default for GetCryptoConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(120),
            format: FormatTemplate::default(),
            reference_currency: "USD".to_string(),
            cryptocurrency: "BTC".to_string(),
        }
    }
}

impl ConfigBlock for GetCrypto {
    type Config = GetCryptoConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        shared_config: SharedConfig,
        _tx_update_request: Sender<Task>,
    ) -> Result<Self> {
        let output = TextWidget::new(id, 0, shared_config);

        Ok(GetCrypto {
            id,
            format: block_config.format.with_default("{currency}: {value:6}")?,
            update_interval: block_config.interval,
            reference_currency: block_config.reference_currency,
            cryptocurrency: block_config.cryptocurrency,
            output,
        })
    }
}

impl Block for GetCrypto {
    fn id(&self) -> usize {
        self.id
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.output]
    }

    fn update(&mut self) -> Result<Option<Update>> {
        let value = get_crypto_value(&self.cryptocurrency, &self.reference_currency)?;
        let formatting_map = map!(
            "value" => Value::from_float(value),
            "currency" => Value::from_string(self.cryptocurrency.clone()),
        );
        self.output.set_texts(self.format.render(&formatting_map)?);
        Ok(Some(self.update_interval.into()))
    }

    fn click(&mut self, _event: &I3BarEvent) -> Result<()> {
        Ok(())
    }
}

fn get_crypto_value(cryptocurrency: &String, reference_currency: &String) -> Result<f64> {
    let pair = match cryptocurrency.to_lowercase().as_str() {
        "btc" => format!("xbt{}", reference_currency.to_lowercase()),
        _ => format!(
            "{}{}",
            cryptocurrency.to_lowercase(),
            reference_currency.to_lowercase()
        ),
    };

    let tickname = match cryptocurrency.to_lowercase().as_str() {
        "btc" => format!("X{}Z{}", "XBT", reference_currency.to_uppercase()),
        other => format!(
            "X{}Z{}",
            other.to_uppercase(),
            reference_currency.to_uppercase()
        ),
    };
    let response = http::http_get_json(
        format!("https://api.kraken.com/0/public/Ticker?pair={}", pair).as_str(),
        Some(Duration::from_secs(3)),
        vec![],
    )?;
    let pairask = response
        .content
        .pointer(format!("/result/{}/a/0", tickname).as_str())
        .and_then(|x| x.as_str())
        .and_then(|x| x.parse::<f64>().ok());
    let pairbid = response
        .content
        .pointer(format!("/result/{}/b/0", tickname).as_str())
        .and_then(|x| x.as_str())
        .and_then(|x| x.parse::<f64>().ok());
    if pairask.is_none() || pairbid.is_none() {
        return Err(BlockError(
            "get_crypto".to_string(),
            "unable to get cryptocurrency pair information".to_string(),
        ));
    }
    return Ok((pairask.unwrap() + pairbid.unwrap()) / 2.0);
}

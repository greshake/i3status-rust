use crate::errors::{Error, ErrorContext, Result, StdError};
use std::borrow::Cow;
use std::fmt;
use std::sync::Arc;
use std::time::Instant;

use serde::Deserialize;
use smart_default::SmartDefault;

mod ip2location;
mod ipapi;

pub struct AutolocateResult {
    pub location: IPAddressInfo,
    pub timestamp: Instant,
}

#[derive(Deserialize, Clone, Default)]
pub struct IPAddressInfo {
    // Required fields
    pub ip: String,
    pub latitude: f64,
    pub longitude: f64,
    pub city: String,

    // Optional fields
    pub version: Option<String>,
    pub region: Option<String>,
    pub region_code: Option<String>,
    pub country: Option<String>,
    pub country_name: Option<String>,
    pub country_code: Option<String>,
    pub country_code_iso3: Option<String>,
    pub country_capital: Option<String>,
    pub country_tld: Option<String>,
    pub continent_code: Option<String>,
    pub in_eu: Option<bool>,
    pub postal: Option<String>,
    pub timezone: Option<String>,
    pub utc_offset: Option<String>,
    pub country_calling_code: Option<String>,
    pub currency: Option<String>,
    pub currency_name: Option<String>,
    pub languages: Option<String>,
    pub country_area: Option<f64>,
    pub country_population: Option<f64>,
    pub asn: Option<String>,
    pub org: Option<String>,
}

#[derive(Deserialize, Debug, SmartDefault, Clone)]
#[serde(tag = "name", rename_all = "lowercase", deny_unknown_fields)]
pub enum Locator {
    #[default]
    Ipapi(ipapi::Config),
    Ip2Location(ip2location::Config),
}

pub trait Backend {
    fn name(&self) -> Cow<'static, str>;
}

impl Backend for Locator {
    fn name(&self) -> Cow<'static, str> {
        match self {
            Locator::Ipapi(_) => ipapi::Ipapi.name(),
            Locator::Ip2Location(_) => ip2location::Ip2Location.name(),
        }
    }
}

impl Locator {
    pub async fn get_info(&self, client: &reqwest::Client) -> Result<IPAddressInfo> {
        match self {
            Locator::Ipapi(_) => ipapi::Ipapi.get_info(client).await,
            Locator::Ip2Location(config) => {
                ip2location::Ip2Location
                    .get_info(client, &config.api_key)
                    .await
            }
        }
    }
}

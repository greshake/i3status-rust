use crate::errors::{Error, ErrorContext, Result, StdError};
use std::borrow::Cow;
use std::fmt;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::Deserialize;
use smart_default::SmartDefault;

mod ip2location;
mod ipapi;

static LAST_AUTOLOCATE: Mutex<Option<AutolocateResult>> = Mutex::new(None);
static LOCATOR: LazyLock<Mutex<Locator>> = LazyLock::new(|| Mutex::new(Locator::default()));

pub(super) const API_KEY_ENV: &str = "AUTOLOCATE_API_KEY";

pub fn get_global_locator_driver_name() -> Cow<'static, str> {
    LOCATOR.lock().unwrap().driver.name()
}

pub fn set_global_locator(locator: Locator) {
    *LOCATOR.lock().unwrap() = locator;
}

struct AutolocateResult {
    location: IPAddressInfo,
    timestamp: Instant,
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

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct Locator {
    pub driver: LocatorDriver,
    #[serde(default = "getenv_api_key")]
    api_key: Option<String>,
}

fn getenv_api_key() -> Option<String> {
    std::env::var(API_KEY_ENV).ok()
}

#[derive(Deserialize, Debug, SmartDefault, Clone)]
#[serde(rename_all = "lowercase", deny_unknown_fields)]
pub enum LocatorDriver {
    #[default]
    Ipapi,
    Ip2Location,
}

#[async_trait]
pub trait Backend {
    fn name(&self) -> Cow<'static, str>;

    async fn get_info(
        &self,
        client: &reqwest::Client,
        api_key: &Option<String>,
    ) -> Result<IPAddressInfo>;
}

#[async_trait]
impl Backend for LocatorDriver {
    fn name(&self) -> Cow<'static, str> {
        match self {
            LocatorDriver::Ipapi => ipapi::Ipapi.name(),
            LocatorDriver::Ip2Location => ip2location::Ip2Location.name(),
        }
    }

    async fn get_info(
        &self,
        client: &reqwest::Client,
        api_key: &Option<String>,
    ) -> Result<IPAddressInfo> {
        match self {
            LocatorDriver::Ipapi => ipapi::Ipapi.get_info(client, api_key).await,
            LocatorDriver::Ip2Location => ip2location::Ip2Location.get_info(client, api_key).await,
        }
    }
}

/// No-op if last API call was made in the last `interval` seconds.
pub async fn find_ip_location(
    client: &reqwest::Client,
    interval: Duration,
) -> Result<IPAddressInfo> {
    {
        let guard = LAST_AUTOLOCATE.lock().unwrap();
        if let Some(cached) = &*guard {
            if cached.timestamp.elapsed() < interval {
                return Ok(cached.location.clone());
            }
        }
    }

    let locator = LOCATOR.lock().unwrap().clone();
    let location = locator.driver.get_info(client, &locator.api_key).await?;

    {
        let mut guard = LAST_AUTOLOCATE.lock().unwrap();
        *guard = Some(AutolocateResult {
            location: location.clone(),
            timestamp: Instant::now(),
        });
    }

    Ok(location)
}

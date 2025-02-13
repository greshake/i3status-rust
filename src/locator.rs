use crate::errors::{Error, ErrorContext, Result, StdError};
use std::borrow::Cow;
use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Deserialize;
use smart_default::SmartDefault;

mod ip2location;
mod ipapi;

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

pub struct Locator {
    backend: LocatorBackend,
    last_autolocate: Mutex<Option<AutolocateResult>>,
}

impl Locator {
    pub fn new(backend: LocatorBackend) -> Self {
        Self {
            backend,
            last_autolocate: Mutex::new(None),
        }
    }

    pub fn name(&self) -> Cow<'static, str> {
        self.backend.name()
    }

    /// No-op if last API call was made in the last `interval` seconds.
    pub async fn find_ip_location(
        &self,
        client: &reqwest::Client,
        interval: Duration,
    ) -> Result<IPAddressInfo> {
        {
            let guard = self.last_autolocate.lock().unwrap();
            if let Some(cached) = &*guard {
                if cached.timestamp.elapsed() < interval {
                    return Ok(cached.location.clone());
                }
            }
        }

        let location = self.backend.get_info(client).await?;

        {
            let mut guard = self.last_autolocate.lock().unwrap();
            *guard = Some(AutolocateResult {
                location: location.clone(),
                timestamp: Instant::now(),
            });
        }

        Ok(location)
    }
}

#[derive(Deserialize, Debug, SmartDefault, Clone)]
#[serde(tag = "name", rename_all = "lowercase", deny_unknown_fields)]
pub enum LocatorBackend {
    #[default]
    Ipapi(ipapi::Config),
    Ip2Location(ip2location::Config),
}

impl LocatorBackend {
    fn name(&self) -> Cow<'static, str> {
        match self {
            LocatorBackend::Ipapi(_) => ipapi::Ipapi.name(),
            LocatorBackend::Ip2Location(_) => ip2location::Ip2Location.name(),
        }
    }

    async fn get_info(&self, client: &reqwest::Client) -> Result<IPAddressInfo> {
        match self {
            LocatorBackend::Ipapi(_) => ipapi::Ipapi.get_info(client).await,
            LocatorBackend::Ip2Location(config) => {
                ip2location::Ip2Location
                    .get_info(client, &config.api_key)
                    .await
            }
        }
    }
}

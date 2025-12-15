//! Geolocation service
//!
//! This global module can be used to provide geolocation information
//! to blocks that support it.
//!
//! ipapi.co is the default geolocator service.
//!
//! # Configuration
//!
//! # ipapi.co Options
//!
//! Key | Values | Required | Default
//! ----|--------|----------|--------
//! `geolocator` | `ipapi` | Yes | None
//!
//! # Ip2Location.io Options
//!
//! Key | Values | Required | Default
//! ----|--------|----------|--------
//! `geolocator` | `ip2location` | Yes | None
//! `api_key` | Your Ip2Location.io API key. | No | None
//!
//! An api key is not required to get back basic information from ip2location.io.
//! However, to get more additional information, an api key is required.
//! See [pricing](https://www.ip2location.io/pricing) for more information.
//!
//! The `api_key` option can be omitted from configuration, in which case it
//! can be provided in the environment variable `IP2LOCATION_API_KEY`
//!
//!
//! # Examples
//!
//! Use the default geolocator service:
//!
//! ```toml
//! [geolocator]
//! geolocator = "ipapi"
//! ```
//!
//! Use Ip2Location.io
//!
//! ```toml
//! [geolocator]
//! geolocator = "ip2location"
//! api_key = "XXX"
//! ```

use crate::errors::{Error, ErrorContext as _, Result, StdError};
use std::borrow::Cow;
use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Deserialize;
use smart_default::SmartDefault;

mod ip2location;
mod ipapi;

#[derive(Debug)]
struct AutolocateResult {
    location: IPAddressInfo,
    timestamp: Instant,
}

#[derive(Deserialize, Clone, Default, Debug)]
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

#[derive(Default, Debug, Deserialize)]
#[serde(from = "GeolocatorBackend")]
pub struct Geolocator {
    backend: GeolocatorBackend,
    last_autolocate: Mutex<Option<AutolocateResult>>,
}

impl Geolocator {
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
            if let Some(cached) = &*guard
                && cached.timestamp.elapsed() < interval
            {
                return Ok(cached.location.clone());
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
#[serde(tag = "geolocator", rename_all = "lowercase", deny_unknown_fields)]
pub enum GeolocatorBackend {
    #[default]
    Ipapi(ipapi::Config),
    Ip2Location(ip2location::Config),
}

impl GeolocatorBackend {
    fn name(&self) -> Cow<'static, str> {
        match self {
            GeolocatorBackend::Ipapi(_) => ipapi::Ipapi.name(),
            GeolocatorBackend::Ip2Location(_) => ip2location::Ip2Location.name(),
        }
    }

    async fn get_info(&self, client: &reqwest::Client) -> Result<IPAddressInfo> {
        match self {
            GeolocatorBackend::Ipapi(_) => ipapi::Ipapi.get_info(client).await,
            GeolocatorBackend::Ip2Location(config) => {
                ip2location::Ip2Location
                    .get_info(client, config.api_key.as_ref())
                    .await
            }
        }
    }
}

impl From<GeolocatorBackend> for Geolocator {
    fn from(backend: GeolocatorBackend) -> Self {
        Self {
            backend,
            last_autolocate: Mutex::new(None),
        }
    }
}

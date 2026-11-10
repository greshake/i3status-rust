//! Geolocation service
//!
//! This global module can be used to provide geolocation information
//! to blocks that support it.
//!
//! ipapi.co is the default geolocator service.
//!
//! # Configuration
//!
//! # Common Options
//!
//! Key | Values | Required | Default
//! ----|--------|----------|--------
//! `rate_limit_interval` | Seconds to wait before contacting the service again after it reported rate limiting. Until then, cached results are served and no new requests are made. | No | `600`
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

use backon::{ExponentialBuilder, Retryable as _};

use crate::errors::{Error, ErrorContext as _, Result, StdError};
use crate::wrappers::Seconds;
use std::borrow::Cow;
use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Deserialize;
use smart_default::SmartDefault;

mod ip2location;
mod ipapi;

/// Per-request timeout, deliberately much shorter than the shared client's
/// 10s: lookups normally take well under a second, and when a request is sent
/// while routes are still changing (the common case for the external_ip
/// block) it just hangs, so failing fast and retrying beats waiting.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug)]
struct AutolocateResult {
    location: IPAddressInfo,
    timestamp: Instant,
}

/// Error cause set by backends when the service refuses to answer because of
/// rate limiting. Callers can detect it with [`is_rate_limited`] and back off
/// instead of retrying, which would only prolong the block.
#[derive(Debug, Clone, Copy)]
pub struct RateLimited;

impl fmt::Display for RateLimited {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("rate limited by the geolocation service")
    }
}

impl StdError for RateLimited {}

pub fn is_rate_limited(err: &Error) -> bool {
    err.cause
        .as_ref()
        .is_some_and(|cause| cause.downcast_ref::<RateLimited>().is_some())
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

#[derive(Debug, Deserialize)]
#[serde(from = "GeolocatorConfig")]
pub struct Geolocator {
    backend: GeolocatorBackend,
    rate_limit_interval: Duration,
    last_autolocate: Mutex<Option<AutolocateResult>>,
    last_rate_limited: Mutex<Option<Instant>>,
}

impl Default for Geolocator {
    fn default() -> Self {
        GeolocatorConfig::default().into()
    }
}

impl Geolocator {
    pub fn name(&self) -> Cow<'static, str> {
        self.backend.name()
    }

    pub fn rate_limit_interval(&self) -> Duration {
        self.rate_limit_interval
    }

    /// No-op if last API call was made in the last `interval` seconds.
    ///
    /// Transient errors are retried with exponential backoff, so callers
    /// don't need their own retry logic. If the service reported rate
    /// limiting less than `rate_limit_interval` seconds ago, no request is
    /// made and an error with a [`RateLimited`] cause is returned, so that
    /// all callers collectively respect the limit.
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

        {
            let guard = self.last_rate_limited.lock().unwrap();
            if let Some(at) = *guard
                && at.elapsed() < self.rate_limit_interval
            {
                return Err(Error {
                    message: Some("geolocation service is rate limited, backing off".into()),
                    cause: Some(Arc::new(RateLimited)),
                });
            }
        }

        let fetch = || self.backend.get_info(client);
        let location = match fetch
            .retry(ExponentialBuilder::default())
            .when(|err| !is_rate_limited(err))
            .await
        {
            Ok(location) => location,
            Err(err) => {
                if is_rate_limited(&err) {
                    *self.last_rate_limited.lock().unwrap() = Some(Instant::now());
                }
                return Err(err);
            }
        };

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

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(default)]
pub struct GeolocatorConfig {
    #[serde(flatten)]
    backend: GeolocatorBackend,
    /// How long to wait before contacting the service again after it reported
    /// rate limiting.
    #[default(600.into())]
    rate_limit_interval: Seconds,
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

impl From<GeolocatorConfig> for Geolocator {
    fn from(config: GeolocatorConfig) -> Self {
        Self {
            backend: config.backend,
            rate_limit_interval: config.rate_limit_interval.0,
            last_autolocate: Mutex::new(None),
            last_rate_limited: Mutex::new(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_config() {
        let geolocator: Geolocator = toml::from_str("geolocator = \"ipapi\"").unwrap();
        assert!(matches!(geolocator.backend, GeolocatorBackend::Ipapi(_)));
        assert_eq!(geolocator.rate_limit_interval, Duration::from_secs(600));

        // the `geolocator` key is required when the [geolocator] section is
        // present (serde's struct-level default does not extend to the
        // flattened backend enum)
        assert!(toml::from_str::<Geolocator>("").is_err());

        let geolocator: Geolocator =
            toml::from_str("geolocator = \"ipapi\"\nrate_limit_interval = 120").unwrap();
        assert_eq!(geolocator.rate_limit_interval, Duration::from_secs(120));

        let geolocator: Geolocator =
            toml::from_str("geolocator = \"ip2location\"\napi_key = \"xxx\"").unwrap();
        let GeolocatorBackend::Ip2Location(config) = geolocator.backend else {
            panic!("wrong backend");
        };
        assert_eq!(config.api_key.as_deref(), Some("xxx"));

        assert!(toml::from_str::<Geolocator>("geolocator = \"ipapi\"\nbad_key = 1").is_err());
    }
}

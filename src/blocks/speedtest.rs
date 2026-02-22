//! Ping, jitter,download, and upload speeds
//!
//! This block uses Cloudflare's [networkquality-rs](https://github.com/cloudflare/networkquality-rs) (nq) library to run a speedtest and report the ping, jitter, download speed, and upload speed.
//!
//! The block can be configured to use custom endpoints for the speedtest, but by default Cloudflare's nq endpoints are used.
//!
//!  For example setting `config_url` to `"https://mensura.cdn-apple.com/.well-known/nq"` will use Apple's nq endpoints instead of Cloudflare's.
//!
//! nq is based on the IETF draft: ["Responsiveness under Working Conditions"](https://datatracker.ietf.org/doc/html/draft-ietf-ippm-responsiveness-03).
//!
//! The draft defines "responsiveness", measured in **R**ound trips **P**er **M**inute (RPM), as a useful measurement of network quality.
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `format` | A string to customise the output of this block. See below for available placeholders. | `" ^icon_ping $ping ^icon_net_down $speed_down ^icon_net_up $speed_up "`
//! `interval` | Update interval in seconds | `1800`
//! `config_url` | The endpoint to get the responsiveness config from. See [`SpeedtestArgs::config_url`] for the expected format of the configuration JSON returned by this endpoint. | `None`
//! `large_download_url` | The large file endpoint which should be multiple GBs. | `"https://h3.speed.cloudflare.com/__down?bytes=10000000000"`
//! `small_download_url` | The small file endpoint which should be very small, only a few bytes. | `"https://h3.speed.cloudflare.com/__down?bytes=10"`
//! `upload_url` | The upload url which accepts an arbitrary amount of data. | `"https://h3.speed.cloudflare.com/__up"`
//! `moving_average_distance` | The number of intervals to use when calculating the moving average. | `4`
//! `std_tolerance` | How far a measurement is allowed to be from the previous moving average before the measurement is considered unstable. | `0.05`
//! `trimmed_mean_percent` | Determines which percentile to use for averaging when calculating the trimmed mean of throughputs or RPM scores. A value of `0.95` means to only use values in the 95th percentile to calculate an average. | `0.95`
//! `max_loaded_connections` | The maximum number of loaded connections that the test can use to saturate the network. | `16`
//! `interval_duration_ms` | The duration between test intervals in milliseconds (ms). | `500` (0.5 seconds)
//! `test_duration_ms` | The overall test duration in milliseconds (ms). | `12_000` (12 seconds)
//!
//! Placeholder  | Value          | Type   | Unit
//! -------------|----------------|--------|---------------
//! `ping`       | Ping delay     | Number | Seconds
//! `jitter`     | Jitter         | Number | Seconds
//! `speed_down` | Download speed | Number | Bits per second
//! `speed_up`   | Upload speed   | Number | Bits per second
//!
//! # Example
//!
//! Show only ping (with an icon)
//!
//! ```toml
//! [[block]]
//! block = "speedtest"
//! interval = 1800
//! format = " ^icon_ping $ping "
//! ```
//!
//! Hide ping and display speed in bytes per second each using 4 characters (without icons)
//!
//! ```toml
//! [[block]]
//! block = "speedtest"
//! interval = 1800
//! format = " $speed_down.eng(w:4,u:B) $speed_up(w:4,u:B) "
//! ```
//!
//! # Icons Used
//! - `ping`
//! - `net_down`
//! - `net_up`

use std::sync::Arc;

use nq_core::{Network, Time, TokioTime};
use nq_latency::{Latency, LatencyConfig, LatencyResult};
use nq_rpm::{Responsiveness, ResponsivenessConfig, ResponsivenessResult};
use nq_tokio_network::TokioNetwork;
use reqwest::Url;
use serde::{Deserialize, Deserializer};
use tokio_util::sync::CancellationToken;

use super::prelude::*;

make_log_macro!(debug, "speedtest");

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(default)]
pub struct Config {
    pub format: FormatConfig,
    #[default(1800.into())]
    pub interval: Seconds,
    #[serde(flatten)]
    pub speedtest_args: SpeedtestArgs,
}

#[derive(Debug, Deserialize, SmartDefault)]
#[serde(deny_unknown_fields, default)]
pub struct SpeedtestArgs {
    /// The endpoint to get the responsiveness config from. Should be JSON in
    /// the form:
    ///
    /// ```json
    /// {
    ///     "version": number,
    ///     "test_endpoint": string?,
    ///     "urls": {
    ///         "small_https_download_url": string,
    ///         "large_https_download_url": string,
    ///         "https_upload_url": string
    ///     }
    /// }
    /// ```
    #[serde(deserialize_with = "deserialize_url_opt")]
    pub config_url: Option<Url>,
    /// The large file endpoint which should be multiple GBs.
    #[default("https://h3.speed.cloudflare.com/__down?bytes=10000000000".parse().unwrap())]
    pub large_download_url: Url,
    /// The small file endpoint which should be very small, only a few bytes.
    #[default("https://h3.speed.cloudflare.com/__down?bytes=10".parse().unwrap())]
    #[serde(deserialize_with = "deserialize_url")]
    pub small_download_url: Url,
    /// The upload url which accepts an arbitrary amount of data.
    #[default("https://h3.speed.cloudflare.com/__up".parse().unwrap())]
    #[serde(deserialize_with = "deserialize_url")]
    pub upload_url: Url,
    /// The number of intervals to use when calculating the moving average.
    #[default(4)]
    pub moving_average_distance: usize,
    /// How far a measurement is allowed to be from the previous moving average
    /// before the measurement is considered unstable.
    #[default(0.05)]
    pub std_tolerance: f64,
    /// Determines which percentile to use for averaging when calculating the
    /// trimmed mean of throughputs or RPM scores. A value of `0.95` means to
    /// only use values in the 95th percentile to calculate an average.
    #[default(0.95)]
    pub trimmed_mean_percent: f64,
    /// The maximum number of loaded connections that the test can use to
    /// saturate the network.
    #[default(16)]
    pub max_loaded_connections: usize,
    /// The duration between test intervals.
    #[default(Duration::from_millis(500))]
    #[serde(deserialize_with = "deserialize_duration_ms")]
    pub interval_duration_ms: Duration,
    /// The overall test duration.
    #[default(Duration::from_millis(12_000))]
    #[serde(deserialize_with = "deserialize_duration_ms")]
    pub test_duration_ms: Duration,
}

pub async fn run(config: &Config, api: &CommonApi) -> Result<()> {
    let format = config.format.with_default(
        " ^icon_ping $ping.eng(prefix:m) ^icon_net_down $speed_down ^icon_net_up $speed_up ",
    )?;

    debug!("{:?}", config.speedtest_args);

    loop {
        let results = run_speedtest(&config.speedtest_args).await?;

        let mut widget = Widget::new().with_format(format.clone());
        widget.set_values(map! {
            "ping" => Value::seconds(results.unloaded_latency_seconds),
            "jitter" => Value::seconds(results.jitter_seconds),
            "speed_down" => Value::bits(results.download),
            "speed_up" => Value::bits(results.upload),
        });
        api.set_widget(widget)?;

        select! {
            _ = sleep(config.interval.0) => (),
            _ = api.wait_for_update_request() => (),
        }
    }
}
#[derive(Debug, Deserialize)]
struct RpmUrls {
    #[serde(alias = "small_download_url", deserialize_with = "deserialize_url")]
    small_https_download_url: Url,
    #[serde(alias = "large_download_url", deserialize_with = "deserialize_url")]
    large_https_download_url: Url,
    #[serde(alias = "upload_url", deserialize_with = "deserialize_url")]
    https_upload_url: Url,
}

#[derive(Deserialize)]
struct RpmServerConfig {
    urls: RpmUrls,
}

/// Run a responsiveness test.
async fn run_speedtest(cli_config: &SpeedtestArgs) -> Result<Report> {
    debug!("running responsiveness test");

    let rpm_urls = match cli_config.config_url.clone() {
        Some(config_url) => {
            debug!("fetching configuration from {config_url}");
            let urls = REQWEST_CLIENT
                .get(config_url)
                .send()
                .await
                .error("Failed to send request with reqwest")?
                .json::<RpmServerConfig>()
                .await
                .error("Failed to parse JSON from rpm config endpoint")?
                .urls;
            debug!("retrieved configuration urls: {urls:?}");

            urls
        }
        None => {
            let urls = RpmUrls {
                small_https_download_url: cli_config.small_download_url.clone(),
                large_https_download_url: cli_config.large_download_url.clone(),
                https_upload_url: cli_config.upload_url.clone(),
            };
            debug!("using default configuration urls: {urls:?}");

            urls
        }
    };

    // first get unloaded RTT measurements
    debug!("determining unloaded latency");
    let rtt_result = test_latency(LatencyConfig {
        url: rpm_urls.small_https_download_url.clone(),

        runs: 20,
    })
    .await?;
    debug!(
        "unloaded latency: {:?} s. jitter: {:?} s",
        rtt_result.median(),
        rtt_result.jitter(),
    );

    let config = ResponsivenessConfig {
        large_download_url: rpm_urls.large_https_download_url,
        small_download_url: rpm_urls.small_https_download_url,
        upload_url: rpm_urls.https_upload_url,
        moving_average_distance: cli_config.moving_average_distance,
        interval_duration: cli_config.interval_duration_ms,
        test_duration: cli_config.test_duration_ms,
        trimmed_mean_percent: cli_config.trimmed_mean_percent,
        std_tolerance: cli_config.std_tolerance,
        max_loaded_connections: cli_config.max_loaded_connections,
    };

    debug!("running download test");
    let download_result = test_network_speed(&config, true).await?;

    debug!("running upload test");
    let upload_result = test_network_speed(&config, false).await?;

    debug!("generating report");
    Report::from_rtt_and_rpm_results(&rtt_result, &download_result, &upload_result)
}

async fn test_network_speed(
    config: &ResponsivenessConfig,
    download: bool,
) -> Result<ResponsivenessResult> {
    let shutdown = CancellationToken::new();
    let time: Arc<dyn Time> = Arc::new(TokioTime::new());
    let network: Arc<dyn Network> =
        Arc::new(TokioNetwork::new(Arc::clone(&time), shutdown.clone()));

    let rpm =
        Responsiveness::new(config.clone(), download).map_err(|e| Error::new(e.to_string()))?;
    let result = rpm
        .run_test(network, time, shutdown.clone())
        .await
        .map_err(|e| Error::new(e.to_string()))?;

    debug!("shutting down network speed test");
    let _ = tokio::time::timeout(tokio::time::Duration::from_secs(1), async {
        shutdown.cancel();
    })
    .await;

    Ok(result)
}

async fn test_latency(config: LatencyConfig) -> Result<LatencyResult> {
    let shutdown = CancellationToken::new();
    let time: Arc<dyn Time> = Arc::new(TokioTime::new());
    let network: Arc<dyn Network> =
        Arc::new(TokioNetwork::new(Arc::clone(&time), shutdown.clone()));

    let rtt = Latency::new(config);
    let result = rtt
        .run_test(network, time, shutdown.clone())
        .await
        .map_err(|e| Error::new(e.to_string()))?;

    debug!("shutting down latency test");
    let _ = tokio::time::timeout(tokio::time::Duration::from_secs(1), async {
        shutdown.cancel();
    })
    .await;

    Ok(result)
}

#[derive(Deserialize)]
struct Report {
    //rtt results
    unloaded_latency_seconds: f64,
    jitter_seconds: f64,
    //rpm results
    download: usize,
    upload: usize,
}

impl Report {
    fn from_rtt_and_rpm_results(
        rtt_result: &LatencyResult,
        download_rpm_result: &ResponsivenessResult,
        upload_rpm_result: &ResponsivenessResult,
    ) -> Result<Self> {
        let unloaded_latency_seconds = rtt_result.median().error("no median RTT available")?;
        let jitter_seconds = rtt_result.jitter().error("no jitter available")?;

        let download = download_rpm_result
            .throughput()
            .error("no download throughput available")?;
        let upload = upload_rpm_result
            .throughput()
            .error("no upload throughput available")?;

        Ok(Report {
            unloaded_latency_seconds,
            jitter_seconds,
            download,
            upload,
        })
    }
}

fn deserialize_url_opt<'de, D>(deserializer: D) -> Result<Option<Url>, D::Error>
where
    D: Deserializer<'de>,
{
    let url_opt = Option::<String>::deserialize(deserializer)?;
    url_opt
        .map(|url| url.parse().map_err(serde::de::Error::custom))
        .transpose()
}

fn deserialize_url<'de, D>(deserializer: D) -> Result<Url, D::Error>
where
    D: Deserializer<'de>,
{
    let url = String::deserialize(deserializer)?;
    url.parse().map_err(serde::de::Error::custom)
}

fn deserialize_duration_ms<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: Deserializer<'de>,
{
    let duration_ms = u64::deserialize(deserializer)?;
    Ok(Duration::from_millis(duration_ms))
}

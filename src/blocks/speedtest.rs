//! Ping, jitter, download, and upload speeds
//!
//! This block uses Cloudflare's [networkquality-rs](https://github.com/cloudflare/networkquality-rs) (nq) library to run a speedtest and report the ping, jitter, download speed, and upload speed.
//!
//! The block can be configured to use custom endpoints for the speedtest, but by default Cloudflare's nq endpoints are used.
//!
//!  For example setting `config_url` to `"https://mensura.cdn-apple.com/.well-known/nq"` will use Apple's nq endpoints instead of Cloudflare's.
//!
//! nq is based on the IETF draft: ["Responsiveness under Working Conditions"](https://datatracker.ietf.org/doc/draft-ietf-ippm-responsiveness/).
//!
//! The draft defines "responsiveness", measured in **R**ound trips **P**er **M**inute (RPM), as a useful measurement of network quality.
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `format` | A string to customise the output of this block. See below for available placeholders. | `" ^icon_ping $ping.eng(prefix:m) ^icon_net_down $speed_down ^icon_net_up $speed_up "`
//! `interval` | Update interval in seconds | `1800`
//! `config_url` | The endpoint to get the responsiveness config from. See [`SpeedtestConfig::config_url`] for the expected format of the configuration JSON returned by this endpoint. | `None`
//! `large_download_url` | The large file endpoint which should be multiple GBs. | `"https://h3.speed.cloudflare.com/__down?bytes=10000000000"`
//! `small_download_url` | The small file endpoint which should be very small, only a few bytes. | `"https://h3.speed.cloudflare.com/__down?bytes=10"`
//! `upload_url` | The upload url which accepts an arbitrary amount of data. | `"https://h3.speed.cloudflare.com/__up"`
//! `latency` | Arguments for the latency test. | [See table below](#latency-configuration-settings-used-for-ping-and-jitter)
//! `rpm` | Arguments for the RPM test. | [See table below](#rpm-configuration-settings-used-for-speed_down-and-speed_up)
//!
//! # Latency Configuration (settings used for `ping` and `jitter`)
//!
//! Key | Values | Default
//! ----|--------|--------
//! `runs` | The number of latency test runs to perform. | `20`
//!
//! # RPM Configuration (settings used for `speed_down` and `speed_up`)
//!
//! Key | Values | Default
//! ----|--------|--------
//! `moving_average_distance` | The number of intervals to use when calculating the moving average. | `4`
//! `std_tolerance` | How far a measurement is allowed to be from the previous moving average before the measurement is considered unstable. | `0.05`
//! `trimmed_mean_percent` | Determines which percentile to use for averaging when calculating the trimmed mean of throughputs or RPM scores. A value of `0.95` means to only use values in the 95th percentile to calculate an average. | `0.95`
//! `max_loaded_connections` | The maximum number of loaded connections that the test can use to saturate the network. | `16`
//! `interval_duration_ms` | The duration between test intervals in milliseconds (ms). | `500` (0.5 seconds)
//! `test_duration_ms` | The overall test duration in milliseconds (ms). | `12_000` (12 seconds)
//! `conn_type` | The type of connection to use for the speed test. One of `"h1"`, `"h2"`, or `"h3"` | `"h2"`
//! `upload_bytes_per_request` | The number of bytes to upload per request during the speed test. | `100_000_000`
//!
//! # Available Format Keys
//!
//! Placeholder  | Value          | Type   | Unit
//! -------------|----------------|--------|---------------
//! `ping`       | Ping delay     | Number | Seconds
//! `jitter`     | Jitter         | Number | Seconds
//! `speed_down` | Download speed | Number | Bits per second
//! `speed_up`   | Upload speed   | Number | Bits per second
//!
//! # Examples
//!
//! Show only ping (with an icon)
//!
//! ```toml
//! [[block]]
//! block = "speedtest"
//! format = " ^icon_ping $ping "
//! ```
//!
//! Hide ping and display speed in bytes per second each using 4 characters (without icons)
//!
//! ```toml
//! [[block]]
//! block = "speedtest"
//! format = " $speed_down.eng(w:4,u:B) $speed_up(w:4,u:B) "
//! ```
//!
//! Advanced configuration
//!
//! ```toml
//! [[block]]
//! block = "speedtest"
//! [block.latency]
//! runs = 5
//! [block.rpm]
//! conn_type = "h1"
//! ```
//!
//! # Icons Used
//! - `ping` (`^icon_ping`)
//! - `net_down` (`^icon_net_down`)
//! - `net_up` (`^icon_net_up`)

use std::sync::Arc;

use cf_mach::{
    nq_core::{ConnectionType, Network, Time, TokioTime},
    nq_latency::{Latency, LatencyConfig, LatencyResult},
    nq_rpm::{ConnectionErrorPolicy, Responsiveness, ResponsivenessConfig, ResponsivenessResult},
    nq_tokio_network::TokioNetwork,
};
use reqwest::Url;
use serde::{Deserialize, Deserializer};
use tokio_util::sync::CancellationToken;

use super::prelude::*;

make_log_macro!(debug, "speedtest");

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub format: FormatConfig,
    #[default(1800.into())]
    pub interval: Seconds,
    #[serde(flatten)]
    pub speedtest: SpeedtestConfig,
}

#[derive(Debug, Deserialize, SmartDefault)]
#[serde(deny_unknown_fields, default)]
pub struct SpeedtestConfig {
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
    pub latency: LatencyConfigOpts,
    pub rpm: RpmConfigOpts,
}

#[derive(Debug, Deserialize, SmartDefault)]
#[serde(deny_unknown_fields, default)]
pub struct LatencyConfigOpts {
    /// The number of runs to perform when measuring latency.
    #[default(20)]
    pub runs: usize,
}

#[derive(Debug, Deserialize, SmartDefault)]
#[serde(deny_unknown_fields, default)]
pub struct RpmConfigOpts {
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
    /// Create an HTTP/1.1, HTTP/2, or HTTP/3 connection.
    #[default(ConnectionType::H2)]
    #[serde(deserialize_with = "deserialize_conn_type")]
    pub conn_type: ConnectionType,
    /// Maximum bytes sent in any single upload load-generating request.
    ///
    /// Upload load is generated as a sequence of requests of this size on each
    /// connection, rather than one enormous request, because servers may cap
    /// request body size and reject anything larger with HTTP 413. Such caps
    /// apply per-request, so staying under one here keeps the link loaded
    /// indefinitely without ever tripping it.
    ///
    /// Must be below the smallest such cap on the path, with margin. It has no
    /// effect on connections too slow to send this many bytes within the test
    /// duration, since their first request never completes either way.
    #[default(100_000_000)]
    pub upload_bytes_per_request: usize,
}

pub(crate) fn prepare(config: &Config) -> Result<Arc<BlockPlan>> {
    // The icons (`ping`, `net_down`, `net_up`) are rendered by `^icon_*`
    // format tokens, not icon-valued placeholders, so no icons are declared.
    BlockPlan::new(vec![OutputPlan::new(
        "main",
        config.format.with_default(
            " ^icon_ping $ping.eng(prefix:m) ^icon_net_down $speed_down ^icon_net_up $speed_up ",
        )?,
    )])
}

pub(crate) async fn run(config: &Config, api: &CommonApi, plan: &Arc<BlockPlan>) -> Result<()> {
    let output_main = plan.output("main")?;
    let format = output_main.format();

    let need_ping = format.contains_key("ping");
    let need_jitter = format.contains_key("jitter");
    let need_speed_down = format.contains_key("speed_down");
    let need_speed_up = format.contains_key("speed_up");

    loop {
        let speedtest_urls = get_speedtest_urls(&config.speedtest).await?;

        let mut values = HashMap::new();

        if need_ping || need_jitter {
            debug!("running latency test");

            let latency_results = test_latency(LatencyConfig {
                url: speedtest_urls.small_https_download_url.clone(),
                runs: config.speedtest.latency.runs,
                scoped_headers: None,
            })
            .await?;

            if need_ping {
                values.insert(
                    "ping".into(),
                    Value::seconds(latency_results.median().error("no median RTT available")?),
                );
            }

            if need_jitter {
                values.insert(
                    "jitter".into(),
                    Value::seconds(latency_results.jitter().error("no jitter available")?),
                );
            }
        }

        if need_speed_down || need_speed_up {
            let responsiveness_config = ResponsivenessConfig {
                large_download_url: speedtest_urls.large_https_download_url,
                small_download_url: speedtest_urls.small_https_download_url,
                upload_url: speedtest_urls.https_upload_url,
                moving_average_distance: config.speedtest.rpm.moving_average_distance,
                interval_duration: config.speedtest.rpm.interval_duration_ms,
                test_duration: config.speedtest.rpm.test_duration_ms,
                trimmed_mean_percent: config.speedtest.rpm.trimmed_mean_percent,
                std_tolerance: config.speedtest.rpm.std_tolerance,
                max_loaded_connections: config.speedtest.rpm.max_loaded_connections,
                conn_type: config.speedtest.rpm.conn_type,
                upload_bytes_per_request: config.speedtest.rpm.upload_bytes_per_request,
                // This is false for RPM, but true for a saturation test
                determine_load_only: false,
                on_connection_error: ConnectionErrorPolicy::default(),
                scoped_headers: None,
            };

            if need_speed_down {
                debug!("running download test");
                let download_result = test_network_speed(&responsiveness_config, true).await?;
                values.insert(
                    "speed_down".into(),
                    Value::bits(
                        download_result
                            .throughput()
                            .error("no download throughput available")?,
                    ),
                );
            }

            if need_speed_up {
                debug!("running upload test");
                let upload_result = test_network_speed(&responsiveness_config, false).await?;
                values.insert(
                    "speed_up".into(),
                    Value::bits(
                        upload_result
                            .throughput()
                            .error("no upload throughput available")?,
                    ),
                );
            }
        }

        let mut widget = output_main.new_widget();
        widget.set_values(values);
        api.set_widget(widget)?;

        select! {
            _ = sleep(config.interval.0) => (),
            _ = api.wait_for_update_request() => (),
        }
    }
}

#[derive(Debug, Deserialize)]
struct SpeedtestUrls {
    #[serde(alias = "small_download_url", deserialize_with = "deserialize_url")]
    small_https_download_url: Url,
    #[serde(alias = "large_download_url", deserialize_with = "deserialize_url")]
    large_https_download_url: Url,
    #[serde(alias = "upload_url", deserialize_with = "deserialize_url")]
    https_upload_url: Url,
}

#[derive(Deserialize)]
struct RpmServerConfig {
    urls: SpeedtestUrls,
}

/// Get speedtest urls
async fn get_speedtest_urls(speedtest_config: &SpeedtestConfig) -> Result<SpeedtestUrls> {
    match speedtest_config.config_url.clone() {
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

            Ok(urls)
        }
        None => Ok(SpeedtestUrls {
            small_https_download_url: speedtest_config.small_download_url.clone(),
            large_https_download_url: speedtest_config.large_download_url.clone(),
            https_upload_url: speedtest_config.upload_url.clone(),
        }),
    }
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

fn deserialize_conn_type<'de, D>(deserializer: D) -> Result<ConnectionType, D::Error>
where
    D: Deserializer<'de>,
{
    let conn_type_str = String::deserialize(deserializer)?;
    match conn_type_str.as_str() {
        "h1" => Ok(ConnectionType::H1 { use_tls: true }),
        "h2" => Ok(ConnectionType::H2),
        "h3" => Ok(ConnectionType::H3),
        _ => Err(serde::de::Error::custom(format!(
            "Invalid connection type: {}. Must be one of: h1, h2, h3",
            conn_type_str
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_declares_main_output_without_icon_placeholders() {
        let plan = prepare(&Config::default()).unwrap();
        let declared: Vec<_> = plan.outputs().map(|o| o.id()).collect();
        assert_eq!(declared, ["main"]);
        let output = plan.output("main").unwrap();
        // Every icon this block draws is a `^icon_*` token in the format
        // rather than an icon value, so the icon surface is entirely static.
        assert_eq!(output.output().icon_placeholders().count(), 0);
        assert_eq!(
            output.output().static_icons(),
            ["ping", "net_down", "net_up"]
        );
    }

    #[test]
    fn a_format_without_icons_declares_none() {
        let config = Config {
            format: " $ping ".parse().unwrap(),
            ..Config::default()
        };
        let plan = prepare(&config).unwrap();
        let output = plan.output("main").unwrap();
        assert!(output.output().static_icons().is_empty());
    }

    #[test]
    fn custom_format_is_respected() {
        let config = Config {
            format: " $ping ".parse().unwrap(),
            ..Config::default()
        };
        let plan = prepare(&config).unwrap();
        let output = plan.output("main").unwrap();
        assert!(output.format().contains_key("ping"));
        assert!(!output.format().contains_key("speed_down"));
    }
}

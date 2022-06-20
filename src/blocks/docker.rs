//! Local docker daemon status
//!
//! # Configuration
//!
//! Key | Values | Required | Default
//! ----|--------|----------|--------
//! `interval` | Update interval, in seconds. | No | `5`
//! `format` | A string to customise the output of this block. See below for available placeholders. | No | `"$running.eng(1)"`
//! `socket_path` | The path to the docker socket. | No | `"/var/run/docker.sock"`
//!
//! Key       | Value                          | Type   | Unit
//! ----------|--------------------------------|--------|-----
//! `total`   | Total containers on the host   | Number | -
//! `running` | Containers running on the host | Number | -
//! `stopped` | Containers stopped on the host | Number | -
//! `paused`  | Containers paused on the host  | Number | -
//! `images`  | Total images on the host       | Number | -
//!
//! # Example
//!
//! ```toml
//! [[block]]
//! block = "docker"
//! interval = 2
//! format = "$running/$total"
//! ```
//!
//! # Icons Used
//!
//! - `docker`

use super::prelude::*;
use std::path::Path;
use tokio::net::UnixStream;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
struct DockerConfig {
    #[default(5.into())]
    interval: Seconds,
    format: FormatConfig,
    #[default("/var/run/docker.sock".into())]
    socket_path: ShellString,
}

pub async fn run(config: toml::Value, mut api: CommonApi) -> Result<()> {
    let config = DockerConfig::deserialize(config).config_error()?;
    let mut widget = api
        .new_widget()
        .with_icon("docker")?
        .with_format(config.format.with_default("$running.eng(1)")?);
    let socket_path = config.socket_path.expand()?;

    loop {
        let status = api.recoverable(|| Status::new(&*socket_path)).await?;

        widget.set_values(map! {
            "total" =>   Value::number(status.total),
            "running" => Value::number(status.running),
            "paused" =>  Value::number(status.paused),
            "stopped" => Value::number(status.stopped),
            "images" =>  Value::number(status.images),
        });
        api.set_widget(&widget).await?;

        select! {
            _ = sleep(config.interval.0) => (),
            _ = api.wait_for_update_request() => (),
        }
    }
}

#[derive(Deserialize, Debug)]
struct Status {
    #[serde(rename = "Containers")]
    total: i64,
    #[serde(rename = "ContainersRunning")]
    running: i64,
    #[serde(rename = "ContainersStopped")]
    stopped: i64,
    #[serde(rename = "ContainersPaused")]
    paused: i64,
    #[serde(rename = "Images")]
    images: i64,
}

impl Status {
    async fn new(socket_path: impl AsRef<Path>) -> Result<Self> {
        let socket = UnixStream::connect(socket_path)
            .await
            .error("Failed to connect to socket")?;
        let (mut request_sender, connection) = hyper::client::conn::handshake(socket)
            .await
            .error("Failed to create request sender")?;
        tokio::spawn(connection);
        let request = hyper::Request::builder()
            .header("Host", "localhost")
            .uri("http://api/info")
            .method("GET")
            .body(hyper::Body::empty())
            .error("Failed to create request")?;
        let response = request_sender
            .send_request(request)
            .await
            .error("Failed to get response")?;
        let bytes = hyper::body::to_bytes(response.into_body())
            .await
            .error("Failed to get response bytes")?;
        serde_json::from_slice::<Self>(&bytes).error("Failed to deserialize JSON")
    }
}

//! Local docker daemon status
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `interval` | Update interval, in seconds. | `5`
//! `format` | A string to customise the output of this block. See below for available placeholders. | `" $icon $running.eng(w:1) "`
//! `socket_path` | The path to the docker socket. Supports path expansions e.g. `~`. | `"/var/run/docker.sock"`
//!
//! Key       | Value                          | Type   | Unit
//! ----------|--------------------------------|--------|-----
//! `icon`    | A static icon                  | Icon   | -
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
//! format = " $icon $running/$total "
//! ```
//!
//! # Icons Used
//!
//! - `docker`

use super::prelude::*;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    #[default(5.into())]
    pub interval: Seconds,
    pub format: FormatConfig,
    #[default("/var/run/docker.sock".into())]
    pub socket_path: ShellString,
}

pub async fn run(config: &Config, api: &CommonApi) -> Result<()> {
    let format = config.format.with_default(" $icon $running.eng(w:1) ")?;
    let socket_path = config.socket_path.expand()?;

    let client = reqwest::Client::builder()
        .unix_socket(&*socket_path)
        .build()
        .unwrap();

    loop {
        let status: Status = client
            .get("http://api/info")
            .send()
            .await
            .error("Failed to get response")?
            .json()
            .await
            .error("Failed to deserialize JSON")?;

        let mut widget = Widget::new().with_format(format.clone());
        widget.set_values(map! {
            "icon" => Value::icon("docker"),
            "total" =>   Value::number(status.total),
            "running" => Value::number(status.running),
            "paused" =>  Value::number(status.paused),
            "stopped" => Value::number(status.stopped),
            "images" =>  Value::number(status.images),
        });
        api.set_widget(widget)?;

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

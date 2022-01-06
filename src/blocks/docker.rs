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
use crate::scheduler::Task;
use crate::widgets::text::TextWidget;
use crate::widgets::I3BarWidget;
use crate::widgets::State;

pub struct Docker {
    id: usize,
    text: TextWidget,
    format: FormatTemplate,
    update_interval: Duration,
    socket_path: String,
}

#[derive(Deserialize, Debug, Clone)]
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

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct DockerConfig {
    /// Update interval in seconds
    #[serde(deserialize_with = "deserialize_duration")]
    pub interval: Duration,

    /// Format override
    pub format: FormatTemplate,

    /// Absolute path to docker socket
    pub socket_path: String,
}

impl Default for DockerConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(5),
            format: FormatTemplate::default(),
            socket_path: "/var/run/docker.sock".to_string(),
        }
    }
}

impl ConfigBlock for Docker {
    type Config = DockerConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        shared_config: SharedConfig,
        _: Sender<Task>,
    ) -> Result<Self> {
        let text = TextWidget::new(id, 0, shared_config)
            .with_text("N/A")
            .with_icon("docker")?;
        let path_expanded = shellexpand::full(&block_config.socket_path).map_err(|e| {
            ConfigurationError(
                "docker".to_string(),
                format!(
                    "Failed to expand socket path {}: {}",
                    &block_config.socket_path, e
                ),
            )
        })?;
        Ok(Docker {
            id,
            text,
            format: block_config.format.with_default("{running}")?,
            update_interval: block_config.interval,
            socket_path: path_expanded.to_string(),
        })
    }
}

impl Block for Docker {
    fn update(&mut self) -> Result<Option<Update>> {
        let socket_path = std::path::PathBuf::from(self.socket_path.as_str());
        let output = http::http_get_socket_json(socket_path, "http:/api/info");

        if output.is_err() {
            self.text.set_text("N/A".to_string());
            self.text.set_state(State::Critical);
            return Ok(Some(self.update_interval.into()));
        }

        let status: Status = match serde_json::from_value(output.unwrap().content)
            .block_error("docker", "Failed to parse JSON response.")
        {
            Ok(status) => status,
            Err(e) => {
                self.text.set_state(State::Critical);
                return Err(e);
            }
        };

        let values = map!(
            "total" =>   Value::from_integer(status.total),
            "running" => Value::from_integer(status.running),
            "paused" =>  Value::from_integer(status.paused),
            "stopped" => Value::from_integer(status.stopped),
            "images" =>  Value::from_integer(status.images),
        );

        self.text.set_texts(self.format.render(&values)?);
        self.text.set_state(State::Idle);

        Ok(Some(self.update_interval.into()))
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.text]
    }

    fn id(&self) -> usize {
        self.id
    }
}

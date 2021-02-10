use std::collections::BTreeMap;
use std::time::Duration;

use crossbeam_channel::Sender;
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::Config;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::http;
use crate::input::I3BarEvent;
use crate::scheduler::Task;
use crate::util::FormatTemplate;
use crate::widget::I3BarWidget;
use crate::widgets::text::TextWidget;

pub struct Docker {
    id: usize,
    text: TextWidget,
    format: FormatTemplate,
    update_interval: Duration,
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

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct DockerConfig {
    /// Update interval in seconds
    #[serde(
        default = "DockerConfig::default_interval",
        deserialize_with = "deserialize_duration"
    )]
    pub interval: Duration,

    /// Format override
    #[serde(default = "DockerConfig::default_format")]
    pub format: String,

    #[serde(default = "DockerConfig::default_color_overrides")]
    pub color_overrides: Option<BTreeMap<String, String>>,
}

impl DockerConfig {
    fn default_interval() -> Duration {
        Duration::from_secs(5)
    }

    fn default_format() -> String {
        "{running}%".to_owned()
    }

    fn default_color_overrides() -> Option<BTreeMap<String, String>> {
        None
    }
}

impl ConfigBlock for Docker {
    type Config = DockerConfig;

    fn new(id: usize, block_config: Self::Config, config: Config, _: Sender<Task>) -> Result<Self> {
        let text = TextWidget::new(config, id)
            .with_text("N/A")
            .with_icon("docker");
        Ok(Docker {
            id,
            text,
            format: FormatTemplate::from_string(&block_config.format)
                .block_error("docker", "Invalid format specified")?,
            update_interval: block_config.interval,
        })
    }
}

impl Block for Docker {
    fn update(&mut self) -> Result<Option<Update>> {
        let socket_path = std::path::PathBuf::from("/var/run/docker.sock");
        let output = http::http_get_socket_json(socket_path, "http:/api/info");

        if output.is_err() {
            self.text.set_text("N/A".to_string());
            return Ok(Some(self.update_interval.into()));
        }

        let status: Status = serde_json::from_value(output.unwrap().content)
            .block_error("docker", "Failed to parse JSON response.")?;

        let values = map!(
            "{total}" => format!("{}", status.total),
            "{running}" => format!("{}", status.running),
            "{paused}" => format!("{}", status.paused),
            "{stopped}" => format!("{}", status.stopped),
            "{images}" => format!("{}", status.images)
        );

        self.text.set_text(self.format.render_static_str(&values)?);

        Ok(Some(self.update_interval.into()))
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.text]
    }

    fn click(&mut self, _: &I3BarEvent) -> Result<()> {
        Ok(())
    }

    fn id(&self) -> usize {
        self.id
    }
}

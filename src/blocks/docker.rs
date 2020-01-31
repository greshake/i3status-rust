use crossbeam_channel::Sender;
use serde_derive::Deserialize;
use std::process::Command;
use std::time::Duration;

use crate::blocks::{Block, ConfigBlock};
use crate::config::Config;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::input::I3BarEvent;
use crate::scheduler::Task;
use crate::util::FormatTemplate;
use crate::widget::I3BarWidget;
use crate::widgets::text::TextWidget;

use uuid::Uuid;

pub struct Docker {
    text: TextWidget,
    id: String,
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
}

impl DockerConfig {
    fn default_interval() -> Duration {
        Duration::from_secs(5)
    }
    fn default_format() -> String {
        "{running}%".to_owned()
    }
}

impl ConfigBlock for Docker {
    type Config = DockerConfig;

    fn new(block_config: Self::Config, config: Config, _: Sender<Task>) -> Result<Self> {
        Ok(Docker {
            id: Uuid::new_v4().to_simple().to_string(),
            text: TextWidget::new(config.clone())
                .with_text("N/A")
                .with_icon("docker"),
            format: FormatTemplate::from_string(&block_config.format)
                .block_error("docker", "Invalid format specified")?,
            update_interval: block_config.interval,
        })
    }
}

impl Block for Docker {
    fn update(&mut self) -> Result<Option<Duration>> {
        let output = match Command::new("sh")
            .args(&[
                "-c",
                "curl --fail --unix-socket /var/run/docker.sock http:/api/info",
            ])
            .output()
        {
            Ok(raw_output) => {
                String::from_utf8(raw_output.stdout).block_error("docker", "Failed to decode")?
            }
            Err(_) => {
                // We don't want the bar to crash if we can't reach the docker daemon.
                self.text.set_text("N/A".to_string());
                return Ok(Some(self.update_interval));
            }
        };

        if output.is_empty() {
            self.text.set_text("N/A".to_string());
            return Ok(Some(self.update_interval));
        }

        let status: Status = serde_json::from_str(&output)
            .block_error("docker", "Failed to parse JSON response.")?;

        let values = map!(
            "{total}" => format!("{}", status.total),
            "{running}" => format!("{}", status.running),
            "{paused}" => format!("{}", status.paused),
            "{stopped}" => format!("{}", status.stopped),
            "{images}" => format!("{}", status.images)
        );

        self.text.set_text(self.format.render_static_str(&values)?);

        Ok(Some(self.update_interval))
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.text]
    }

    fn click(&mut self, _: &I3BarEvent) -> Result<()> {
        Ok(())
    }

    fn id(&self) -> &str {
        &self.id
    }
}

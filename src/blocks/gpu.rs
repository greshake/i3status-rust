use std::time::Duration;
use std::process::Command;
use chan::Sender;

use block::{Block, ConfigBlock};
use config::Config;
use de::deserialize_duration;
use errors::*;
use widgets::text::TextWidget;
use widget::I3BarWidget;
//use input::I3BarEvent;
use scheduler::Task;

use uuid::Uuid;

pub struct Gpu {
    gpu_widget: TextWidget,
    id: String,
    update_interval: Duration,

    gpu_id: u64,
    utilization: u64,
    /*
    memory_used: u64,
    memory_total: u64,
    temperature: u64,
    fan_speed: u64,
    */
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct GpuConfig {
    /// Update interval in seconds
    #[serde(default = "GpuConfig::default_interval", deserialize_with = "deserialize_duration")]
    pub interval: Duration,

    /// GPU id in system
    #[serde(default = "GpuConfig::default_gpu_id")]
    pub gpu_id: u64,
}

impl GpuConfig {
    fn default_interval() -> Duration {
        Duration::from_secs(5)
    }

    fn default_gpu_id() -> u64 {
        0
    }
}

impl ConfigBlock for Gpu {
    type Config = GpuConfig;

    fn new(block_config: Self::Config, config: Config, _tx_update_request: Sender<Task>) -> Result<Self> {
        Ok(Gpu {
            id: Uuid::new_v4().simple().to_string(),
            update_interval: block_config.interval,
            gpu_widget: TextWidget::new(config.clone()).with_text("gpu"),
            // TODO
            // Add params
            gpu_id: block_config.gpu_id,
            utilization: 66,
        })
    }
}

impl Block for Gpu {
    fn update(&mut self) -> Result<Option<Duration>> {
        //let mut params = vec![];
        let params = "utilization.gpu";
        let output = Command::new("nvidia-smi")
            .args(
                &[
                    "-i", &self.gpu_id.to_string(),
                    &format!("--query-gpu={}", params),
                    "--format=csv,noheader,nounits"
                ],
            )
            .output()
            .block_error("gpu", "Failed to execute nvidia-smi.")?
            .stdout;
        self.utilization = 0;
        self.gpu_widget.set_text(String::from_utf8(output)
            .block_error("net", "Non-UTF8 bitrate.")
            .unwrap());
        Ok(Some(self.update_interval))
    }

    fn view(&self) -> Vec<&I3BarWidget> {
        vec![&self.gpu_widget]
    }

    /*
    fn click(&mut self, _: &I3BarEvent) -> Result<()> {
        Ok(())
    }
    */

    fn id(&self) -> &str {
        &self.id
    }
}

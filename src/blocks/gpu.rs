use std::time::Duration;
use std::process::Command;
use chan::Sender;

use block::{Block, ConfigBlock};
use config::Config;
use de::deserialize_duration;
use errors::*;
use widgets::text::TextWidget;
use widget::I3BarWidget;
use scheduler::Task;

use uuid::Uuid;

pub struct Gpu {
    gpu_widget: TextWidget,
    id: String,
    update_interval: Duration,

    gpu_id: u64,
    vendor: String,
    driver: String,
    label: String,
    utilization: Option<TextWidget>,
    memory: Option<TextWidget>,
    temperature: Option<TextWidget>,
    fan_speed: Option<TextWidget>,
    clocks: Option<TextWidget>,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct GpuConfig {
    /// Update interval in seconds
    #[serde(default = "GpuConfig::default_interval", deserialize_with = "deserialize_duration")]
    pub interval: Duration,

    /// Label
    #[serde(default = "GpuConfig::default_label")]
    pub label: String,

    /// Vendor
    #[serde(default = "GpuConfig::default_vendor")]
    pub vendor: String,

    /// Driver version
    #[serde(default = "GpuConfig::default_driver")]
    pub driver: String,

    /// GPU id in system
    #[serde(default = "GpuConfig::default_gpu_id")]
    pub gpu_id: u64,

    /// GPU utilization. In percents.
    #[serde(default = "GpuConfig::default_utilization")]
    pub utilization: bool,

    /// VRAM utilization.
    #[serde(default = "GpuConfig::default_memory")]
    pub memory: bool,

    /// Core GPU temperature. In degrees C.
    #[serde(default = "GpuConfig::default_temperature")]
    pub temperature: bool,

    /// Fan speed.
    #[serde(default = "GpuConfig::default_fan_speed")]
    pub fan_speed: bool,

    /// GPU clocks. In percents.
    #[serde(default = "GpuConfig::default_clocks")]
    pub clocks: bool,
}

impl GpuConfig {
    fn default_interval() -> Duration {
        Duration::from_secs(5)
    }

    fn default_label() -> String {
        "".to_string()
    }

    fn default_vendor() -> String {
        "nvidia".to_string()
    }

    fn default_driver() -> String {
        "closed".to_string()
    }

    fn default_gpu_id() -> u64 {
        0
    }

    fn default_utilization() -> bool {
        true
    }

    fn default_memory() -> bool {
        true
    }

    fn default_temperature() -> bool {
        true
    }

    fn default_fan_speed() -> bool {
        true
    }

    fn default_clocks() -> bool {
        true
    }
}

impl ConfigBlock for Gpu {
    type Config = GpuConfig;

    fn new(block_config: Self::Config, config: Config, _tx_update_request: Sender<Task>) -> Result<Self> {
        Ok(Gpu {
            id: Uuid::new_v4().simple().to_string(),
            update_interval: block_config.interval,
            gpu_widget: TextWidget::new(config.clone()).with_icon("gpu"),
            // TODO
            // Add open source drivers
            gpu_id: block_config.gpu_id,
            vendor: block_config.vendor,
            driver: block_config.driver,
            label: block_config.label,
            utilization: match block_config.utilization {
                true => Some(TextWidget::new(config.clone())),
                false => None,
            },
            memory: match block_config.memory {
                true => Some(TextWidget::new(config.clone())),
                false => None,
            },
            temperature: match block_config.temperature {
                true => Some(TextWidget::new(config.clone())),
                false => None,
            },
            fan_speed: match block_config.fan_speed {
                true => Some(TextWidget::new(config.clone())),
                false => None,
            },
            clocks: match block_config.clocks {
                true => Some(TextWidget::new(config.clone())),
                false => None,
            },
        })
    }
}

impl Block for Gpu {
    fn update(&mut self) -> Result<Option<Duration>> {
        // TODO
        // Add open source drivers
        if self.vendor != "nvidia" || self.driver != "closed" {
            self.gpu_widget.set_text(format!("Invalid config. Do not use AMD and open drivers"));

            return Ok(Some(self.update_interval));
        }

        let mut params = String::new();
        if self.utilization.is_some() {
            params += "utilization.gpu,";
        }
        if self.memory.is_some() {
            params += "memory.used,memory.total,";
        }
        if self.temperature.is_some() {
            params += "temperature.gpu,";
        }
        if self.fan_speed.is_some() {
            params += "fan.speed,";
        }
        if self.clocks.is_some() {
            params += "clocks.current.graphics,";
        }

        let mut output = Command::new("nvidia-smi")
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
        output.pop(); // Remove trailing newline.
        let result_str = String::from_utf8(output).unwrap();
        let result: Vec<&str> = result_str.split(", ").collect();

        let mut count: usize = 0;
        if let Some(ref mut utilization_widget) = self.utilization {
            utilization_widget.set_text(format!("{:02}%", result[count]));
            count += 1;
        }
        if let Some(ref mut memory_widget) = self.memory {
            memory_widget.set_text(format!("{}MB/{}MB", result[count], result[count + 1]));
            count += 2;
        }
        if let Some(ref mut temperature_widget) = self.temperature {
            temperature_widget.set_text(format!("{:02}°C", result[count]));
            count += 1;
        }
        if let Some(ref mut fan_speed_widget) = self.fan_speed {
            fan_speed_widget.set_text(format!("{:02}%", result[count]));
            count += 1;
        }
        if let Some(ref mut clocks_widget) = self.clocks {
            clocks_widget.set_text(format!("{}MHz", result[count]));
        }

        self.gpu_widget.set_text(format!("{}", self.label));

        Ok(Some(self.update_interval))
    }

    fn view(&self) -> Vec<&I3BarWidget> {
        let mut widgets: Vec<&I3BarWidget> = Vec::new();
        widgets.push(&self.gpu_widget);
        if let Some(ref utilization_widget) = self.utilization {
            widgets.push(utilization_widget);
        }
        if let Some(ref memory_widget) = self.memory {
            widgets.push(memory_widget);
        }
        if let Some(ref temperature_widget) = self.temperature {
            widgets.push(temperature_widget);
        }
        if let Some(ref fan_speed_widget) = self.fan_speed {
            widgets.push(fan_speed_widget);
        }
        if let Some(ref clocks_widget) = self.clocks {
            widgets.push(clocks_widget);
        }
        widgets
    }

    fn id(&self) -> &str {
        &self.id
    }
}
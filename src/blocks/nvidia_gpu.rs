use std::time::Duration;
use std::process::Command;
use chan::Sender;

use block::{Block, ConfigBlock};
use config::Config;
use de::deserialize_duration;
use errors::*;
use input::{I3BarEvent, MouseButton};
use scheduler::Task;
use uuid::Uuid;
use widget::{I3BarWidget, State};
use widgets::button::ButtonWidget;
use widgets::text::TextWidget;

pub struct NvidiaGpu {
    gpu_widget: ButtonWidget,
    id: String,
    id_fans: String,
    id_memory: String,
    update_interval: Duration,

    gpu_id: u64,
    gpu_name: String,
    gpu_name_displayed: bool,
    label: String,
    show_utilization: Option<TextWidget>,
    show_memory: Option<ButtonWidget>,
    memory_total: String,
    memory_total_displayed: bool,
    show_temperature: Option<TextWidget>,
    show_fan: Option<ButtonWidget>,
    fan_speed: u64,
    fan_speed_controlled: bool,
    show_clocks: Option<TextWidget>,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct NvidiaGpuConfig {
    /// Update interval in seconds
    #[serde(default = "NvidiaGpuConfig::default_interval", deserialize_with = "deserialize_duration")]
    pub interval: Duration,

    /// Label
    #[serde(default = "NvidiaGpuConfig::default_label")]
    pub label: String,

    /// GPU id in system
    #[serde(default = "NvidiaGpuConfig::default_gpu_id")]
    pub gpu_id: u64,

    /// GPU utilization. In percents.
    #[serde(default = "NvidiaGpuConfig::default_show_utilization")]
    pub show_utilization: bool,

    /// VRAM utilization.
    #[serde(default = "NvidiaGpuConfig::default_show_memory")]
    pub show_memory: bool,

    /// Core GPU temperature. In degrees C.
    #[serde(default = "NvidiaGpuConfig::default_show_temperature")]
    pub show_temperature: bool,

    /// Fan speed. In percents.
    #[serde(default = "NvidiaGpuConfig::default_show_fan_speed")]
    pub show_fan_speed: bool,

    /// GPU clocks. In percents.
    #[serde(default = "NvidiaGpuConfig::default_show_clocks")]
    pub show_clocks: bool,
}

impl NvidiaGpuConfig {
    fn default_interval() -> Duration {
        Duration::from_secs(3)
    }

    fn default_label() -> String {
        "".to_string()
    }

    fn default_gpu_id() -> u64 {
        0
    }

    fn default_show_utilization() -> bool {
        true
    }

    fn default_show_memory() -> bool {
        true
    }

    fn default_show_temperature() -> bool {
        true
    }

    fn default_show_fan_speed() -> bool {
        false
    }

    fn default_show_clocks() -> bool {
        false
    }
}

impl ConfigBlock for NvidiaGpu {
    type Config = NvidiaGpuConfig;

    fn new(block_config: Self::Config, config: Config, _tx_update_request: Sender<Task>) -> Result<Self> {
        let id = Uuid::new_v4().simple().to_string();
        let id_memory = Uuid::new_v4().simple().to_string();
        let id_fans = Uuid::new_v4().simple().to_string();
        let mut output = Command::new("nvidia-smi")
            .args(
                &[
                    "-i", &block_config.gpu_id.to_string(),
                    "--query-gpu=name,memory.total",
                    "--format=csv,noheader,nounits"
                ],
            )
            .output()
            .block_error("gpu", "Failed to execute nvidia-smi.")?
            .stdout;
        output.pop(); // Remove trailing newline.
        let result_str = String::from_utf8(output).unwrap();
        let result: Vec<&str> = result_str.split(", ").collect();

        Ok(NvidiaGpu {
            id: id.clone(),
            id_fans: id_fans.clone(),
            id_memory: id_memory.clone(),
            update_interval: block_config.interval,
            gpu_widget: ButtonWidget::new(config.clone(), &id).with_icon("gpu"),

            gpu_name: result[0].to_string(),
            gpu_name_displayed: false,
            gpu_id: block_config.gpu_id,
            label: block_config.label,
            show_utilization: if block_config.show_utilization {
                Some(TextWidget::new(config.clone())) } else {
                None
            },
            show_memory: if block_config.show_memory {
                Some(ButtonWidget::new(config.clone(), &id_memory)) } else {
                None
            },
            memory_total: result[1].to_string(),
            memory_total_displayed: false,
            show_temperature: if block_config.show_temperature {
                Some(TextWidget::new(config.clone())) } else {
                None
            },
            show_fan: if block_config.show_fan_speed {
                Some(ButtonWidget::new(config.clone(), &id_fans)) } else {
                None
            },
            fan_speed: 0,
            fan_speed_controlled: false,
            show_clocks: if block_config.show_clocks {
                 Some(TextWidget::new(config.clone())) } else {
                None
            },
        })
    }
}

impl Block for NvidiaGpu {
    fn update(&mut self) -> Result<Option<Duration>> {
        let mut params = String::new();
        if self.show_utilization.is_some() {
            params += "utilization.gpu,";
        }
        if self.show_memory.is_some() {
            params += "memory.used,";
        }
        if self.show_temperature.is_some() {
            params += "temperature.gpu,";
        }
        if self.show_fan.is_some() {
            params += "fan.speed,";
        }
        if self.show_clocks.is_some() {
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
        // TODO
        // Change to 'retain' in rust 1.26
        let result: Vec<&str> = result_str.split(", ").collect();

        let mut count: usize = 0;
        if let Some(ref mut utilization_widget) = self.show_utilization {
            utilization_widget.set_text(format!("{}%", result[count]));
            count += 1;
        }
        if let Some(ref mut memory_widget) = self.show_memory {
            if self.memory_total_displayed {
                memory_widget.set_text(format!("{}MB", self.memory_total));
            } else {
                memory_widget.set_text(format!("{}MB", result[count]));
            }
            count += 1;
        }
        if let Some(ref mut temperature_widget) = self.show_temperature {
            let temp = result[count].parse::<u64>().unwrap();
            temperature_widget.set_state(match temp {
                0...50 => State::Good,
                51...70 => State::Idle,
                71...75 => State::Info,
                76...80 => State::Warning,
                _ => State::Critical,
            });
            temperature_widget.set_text(format!("{:02}°C", temp));
            count += 1;
        }
        if let Some(ref mut fan_widget) = self.show_fan {
            self.fan_speed = result[count].parse::<u64>().unwrap();
            fan_widget.set_text(format!("{:02}%", self.fan_speed));
            count += 1;
        }
        if let Some(ref mut clocks_widget) = self.show_clocks {
            clocks_widget.set_text(format!("{}MHz", result[count]));
        }

        if self.gpu_name_displayed {
            self.gpu_widget.set_text(self.gpu_name.as_ref());
        } else {
            self.gpu_widget.set_text(self.label.as_ref());
        }

        Ok(Some(self.update_interval))
    }

    fn view(&self) -> Vec<&I3BarWidget> {
        let mut widgets: Vec<&I3BarWidget> = Vec::new();
        widgets.push(&self.gpu_widget);
        if let Some(ref utilization_widget) = self.show_utilization {
            widgets.push(utilization_widget);
        }
        if let Some(ref memory_widget) = self.show_memory {
            widgets.push(memory_widget);
        }
        if let Some(ref temperature_widget) = self.show_temperature {
            widgets.push(temperature_widget);
        }
        if let Some(ref fan_widget) = self.show_fan {
            widgets.push(fan_widget);
        }
        if let Some(ref clocks_widget) = self.show_clocks {
            widgets.push(clocks_widget);
        }
        widgets
    }

    fn click(&mut self, e: &I3BarEvent) -> Result<()> {
        if let Some(ref name) = e.name {
            let event_name = name.as_str();

            if event_name == self.id {
                self.gpu_name_displayed = match e.button {
                    MouseButton::Left => !self.gpu_name_displayed,
                    _ => self.gpu_name_displayed
                };

                if self.gpu_name_displayed {
                    self.gpu_widget.set_text(self.gpu_name.as_ref());
                } else {
                    self.gpu_widget.set_text(self.label.as_ref());
                }
            }

            if event_name == self.id_memory {
                self.memory_total_displayed = match e.button {
                    MouseButton::Left => !self.memory_total_displayed,
                    _ => self.gpu_name_displayed
                };

                if let Some(ref mut memory_widget) = self.show_memory {
                    if self.memory_total_displayed {
                        memory_widget.set_text(format!("{}MB", self.memory_total));
                    } else {
                        let mut output = Command::new("nvidia-smi")
                            .args(
                                &[
                                    "-i", &self.gpu_id.to_string(),
                                    "--query-gpu=memory.used",
                                    "--format=csv,noheader,nounits"
                                ],
                            )
                            .output()
                            .block_error("gpu", "Failed to execute nvidia-smi.")?
                            .stdout;
                        output.pop(); // Remove trailing newline.
                        let result_str = String::from_utf8(output).unwrap();
                        memory_widget.set_text(format!("{}MB", result_str));
                    }
                }
            }

            if event_name == self.id_fans {
                let mut controlled_changed = false;
                let mut new_fan_speed = self.fan_speed;
                match e.button {
                    MouseButton::Left => {
                        self.fan_speed_controlled = !self.fan_speed_controlled;
                        controlled_changed = true;
                    }
                    MouseButton::WheelUp => {
                        if self.fan_speed < 100 && self.fan_speed_controlled {
                            new_fan_speed += 1;
                        }
                    }
                    MouseButton::WheelDown => {
                        if self.fan_speed > 0 && self.fan_speed_controlled {
                            new_fan_speed -= 1;
                        }
                    }
                    _ => {}
                };

                if let Some(ref mut fan_widget) = self.show_fan {
                    if controlled_changed {
                        if self.fan_speed_controlled {
                            Command::new("nvidia-settings")
                                .args(
                                    &[
                                        "-a",
                                        &format!("[gpu:{}]/GPUFanControlState=1",
                                                 self.gpu_id),
                                        "-a",
                                        &format!("[fan:{}]/GPUTargetFanSpeed={}",
                                                 self.gpu_id,
                                                 self.fan_speed),
                                    ],
                                )
                                .output()
                                .block_error("gpu", "Failed to execute nvidia-settings.")?;
                            fan_widget.set_text(format!("{:02}%", self.fan_speed));
                            fan_widget.set_state(State::Warning);
                        } else {
                            Command::new("nvidia-settings")
                                .args(
                                    &[
                                        "-a",
                                        &format!("[gpu:{}]/GPUFanControlState=0",
                                                 self.gpu_id),
                                    ],
                                )
                                .output()
                                .block_error("gpu", "Failed to execute nvidia-settings.")?;
                            fan_widget.set_state(State::Idle);
                        }
                    } else if self.fan_speed_controlled {
                        Command::new("nvidia-settings")
                            .args(
                                &[
                                    "-a",
                                    &format!("[fan:{}]/GPUTargetFanSpeed={}",
                                             self.gpu_id,
                                             new_fan_speed),
                                ],
                            )
                            .output()
                            .block_error("gpu", "Failed to execute nvidia-settings.")?;
                        self.fan_speed = new_fan_speed;
                        fan_widget.set_text(format!("{:02}%", new_fan_speed));
                    }
                }
            }
        }

        Ok(())
    }

    fn id(&self) -> &str {
        &self.id
    }
}

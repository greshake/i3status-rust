use std::collections::BTreeMap;
use std::process::Command;
use std::time::Duration;

use crossbeam_channel::Sender;
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::{Config, LogicalDirection, Scrolling};
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::input::{I3BarEvent, MouseButton};
use crate::scheduler::Task;
use crate::util::pseudo_uuid;
use crate::widget::{I3BarWidget, Spacing, State};
use crate::widgets::button::ButtonWidget;
use crate::widgets::text::TextWidget;

pub struct NvidiaGpu {
    id: usize,
    id_fans: usize,
    id_memory: usize,
    update_interval: Duration,

    gpu_enabled: bool,
    gpu_id: u64,

    name_widget: ButtonWidget,
    name_widget_mode: NameWidgetMode,
    label: String,

    show_memory: Option<ButtonWidget>,
    memory_widget_mode: MemoryWidgetMode,

    show_utilization: Option<TextWidget>,
    show_temperature: Option<TextWidget>,

    show_fan: Option<ButtonWidget>,
    fan_speed: u64,
    fan_speed_controlled: bool,
    scrolling: Scrolling,

    show_clocks: Option<TextWidget>,
    maximum_idle: u64,
    maximum_good: u64,
    maximum_info: u64,
    maximum_warning: u64,
}

enum MemoryWidgetMode {
    ShowUsedMemory,
    ShowTotalMemory,
}

enum NameWidgetMode {
    ShowDefaultName,
    ShowLabel,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct NvidiaGpuConfig {
    /// Update interval in seconds
    #[serde(
        default = "NvidiaGpuConfig::default_interval",
        deserialize_with = "deserialize_duration"
    )]
    pub interval: Duration,

    /// Label to show instead of the default GPU name from `nvidia-smi`
    #[serde(default = "NvidiaGpuConfig::default_label")]
    pub label: Option<String>,

    /// GPU id in system
    #[serde(default = "NvidiaGpuConfig::default_gpu_id")]
    pub gpu_id: u64,

    /// GPU utilization. In percent.
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

    /// Maximum temperature, below which state is set to idle
    #[serde(default = "NvidiaGpuConfig::default_idle")]
    pub idle: u64,

    /// Maximum temperature, below which state is set to good
    #[serde(default = "NvidiaGpuConfig::default_good")]
    pub good: u64,

    /// Maximum temperature, below which state is set to info
    #[serde(default = "NvidiaGpuConfig::default_info")]
    pub info: u64,

    /// Maximum temperature, below which state is set to warning
    #[serde(default = "NvidiaGpuConfig::default_warning")]
    pub warning: u64,

    #[serde(default = "NvidiaGpuConfig::default_color_overrides")]
    pub color_overrides: Option<BTreeMap<String, String>>,
}

impl NvidiaGpuConfig {
    fn default_interval() -> Duration {
        Duration::from_secs(3)
    }

    fn default_label() -> Option<String> {
        None
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

    fn default_idle() -> u64 {
        50
    }

    fn default_good() -> u64 {
        70
    }

    fn default_info() -> u64 {
        75
    }

    fn default_warning() -> u64 {
        80
    }

    fn default_color_overrides() -> Option<BTreeMap<String, String>> {
        None
    }
}

impl ConfigBlock for NvidiaGpu {
    type Config = NvidiaGpuConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        config: Config,
        _tx_update_request: Sender<Task>,
    ) -> Result<Self> {
        let id_memory = pseudo_uuid();
        let id_fans = pseudo_uuid();

        Ok(NvidiaGpu {
            id,
            id_fans,
            id_memory,
            update_interval: block_config.interval,
            gpu_enabled: false,
            gpu_id: block_config.gpu_id,

            name_widget: ButtonWidget::new(config.clone(), id)
                .with_icon("gpu")
                .with_spacing(Spacing::Inline),
            name_widget_mode: if block_config.label.is_some() {
                NameWidgetMode::ShowLabel
            } else {
                NameWidgetMode::ShowDefaultName
            },
            label: if block_config.label.is_some() {
                block_config.label.unwrap()
            } else {
                "".to_string()
            },

            show_memory: if block_config.show_memory {
                Some(ButtonWidget::new(config.clone(), id_memory).with_spacing(Spacing::Inline))
            } else {
                None
            },
            memory_widget_mode: MemoryWidgetMode::ShowUsedMemory,

            show_utilization: if block_config.show_utilization {
                Some(TextWidget::new(config.clone(), id).with_spacing(Spacing::Inline))
            } else {
                None
            },

            show_temperature: if block_config.show_temperature {
                Some(TextWidget::new(config.clone(), id).with_spacing(Spacing::Inline))
            } else {
                None
            },

            show_fan: if block_config.show_fan_speed {
                Some(ButtonWidget::new(config.clone(), id_fans).with_spacing(Spacing::Inline))
            } else {
                None
            },
            fan_speed: 0,
            fan_speed_controlled: false,
            scrolling: config.scrolling,

            show_clocks: if block_config.show_clocks {
                Some(TextWidget::new(config, id).with_spacing(Spacing::Inline))
            } else {
                None
            },

            maximum_idle: block_config.idle,
            maximum_good: block_config.good,
            maximum_info: block_config.info,
            maximum_warning: block_config.warning,
        })
    }
}

impl Block for NvidiaGpu {
    fn update(&mut self) -> Result<Option<Update>> {
        let mut params = String::from("name,memory.total,");
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

        let handle = Command::new("nvidia-smi")
            .args(&[
                "-i",
                &self.gpu_id.to_string(),
                &format!("--query-gpu={}", params),
                "--format=csv,noheader,nounits",
            ])
            .output()
            .block_error("gpu", "Failed to execute nvidia-smi.")?;

        self.gpu_enabled = match handle.status.code() {
            Some(0) => true,
            Some(9) => false,
            Some(code) => {
                return Err(BlockError(
                    "nvidia_gpu".to_string(),
                    format!("nvidia-smi error code {}", code),
                ))
            }
            None => {
                return Err(BlockError(
                    "nvidia_gpu".to_string(),
                    "nvidia-smi terminated by signal".to_string(),
                ))
            }
        };

        if self.gpu_enabled {
            let mut output = handle.stdout;
            output.pop(); // Remove trailing newline.
            let result_str = String::from_utf8(output).unwrap();
            // TODO: Change to 'retain' in rust 1.26
            let result: Vec<&str> = result_str.split(", ").collect();

            let gpu_name = result[0].to_string();
            let memory_total = result[1].to_string();

            match self.name_widget_mode {
                NameWidgetMode::ShowDefaultName => {
                    self.name_widget.set_text(gpu_name);
                    self.name_widget.set_spacing(Spacing::Inline);
                }
                NameWidgetMode::ShowLabel => {
                    if self.label.is_empty() {
                        self.name_widget.set_spacing(Spacing::Hidden);
                    } else {
                        self.name_widget.set_spacing(Spacing::Inline);
                    }
                    self.name_widget.set_text(self.label.to_string());
                }
            }

            let mut count: usize = 2;
            if let Some(ref mut utilization_widget) = self.show_utilization {
                utilization_widget.set_text(format!("{:02}%", result[count]));
                count += 1;
            }
            if let Some(ref mut memory_widget) = self.show_memory {
                match self.memory_widget_mode {
                    MemoryWidgetMode::ShowUsedMemory => {
                        memory_widget.set_text(format!("{}MB", result[count]));
                    }
                    MemoryWidgetMode::ShowTotalMemory => {
                        memory_widget.set_text(format!("{}MB", memory_total));
                    }
                }
                count += 1;
            }
            if let Some(ref mut temperature_widget) = self.show_temperature {
                let temp = result[count].parse::<u64>().unwrap_or(0);
                temperature_widget.set_state(match temp {
                    t if t <= self.maximum_idle => State::Idle,
                    t if t <= self.maximum_good => State::Good,
                    t if t <= self.maximum_info => State::Info,
                    t if t <= self.maximum_warning => State::Warning,
                    _ => State::Critical,
                });
                temperature_widget.set_text(format!("{:02}°C", temp));
                count += 1;
            }
            if let Some(ref mut fan_widget) = self.show_fan {
                self.fan_speed = result[count].parse::<u64>().unwrap_or(0);
                fan_widget.set_text(format!("{:02}%", self.fan_speed));
                count += 1;
            }
            if let Some(ref mut clocks_widget) = self.show_clocks {
                clocks_widget.set_text(format!("{}MHz", result[count]));
            }
        } else {
            self.name_widget.set_text("DISABLED".to_string());
        }

        Ok(Some(self.update_interval.into()))
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        let mut widgets: Vec<&dyn I3BarWidget> = Vec::new();
        widgets.push(&self.name_widget);

        if self.gpu_enabled {
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
        }
        widgets
    }

    fn click(&mut self, e: &I3BarEvent) -> Result<()> {
        if let Some(event_id) = e.id {
            if event_id == self.id {
                if let MouseButton::Left = e.button {
                    match self.name_widget_mode {
                        NameWidgetMode::ShowDefaultName => {
                            self.name_widget_mode = NameWidgetMode::ShowLabel
                        }
                        NameWidgetMode::ShowLabel => {
                            self.name_widget_mode = NameWidgetMode::ShowDefaultName
                        }
                    }
                    self.update()?;
                }
            }

            if event_id == self.id_memory {
                if let MouseButton::Left = e.button {
                    match self.memory_widget_mode {
                        MemoryWidgetMode::ShowUsedMemory => {
                            self.memory_widget_mode = MemoryWidgetMode::ShowTotalMemory
                        }
                        MemoryWidgetMode::ShowTotalMemory => {
                            self.memory_widget_mode = MemoryWidgetMode::ShowUsedMemory
                        }
                    }
                    self.update()?;
                }
            }

            if event_id == self.id_fans {
                let mut controlled_changed = false;
                let mut new_fan_speed = self.fan_speed;
                match e.button {
                    MouseButton::Left => {
                        self.fan_speed_controlled = !self.fan_speed_controlled;
                        controlled_changed = true;
                    }
                    _ => {
                        use LogicalDirection::*;
                        match self.scrolling.to_logical_direction(e.button) {
                            Some(Up) => {
                                if self.fan_speed < 100 && self.fan_speed_controlled {
                                    new_fan_speed += 1;
                                }
                            }
                            Some(Down) => {
                                if self.fan_speed > 0 && self.fan_speed_controlled {
                                    new_fan_speed -= 1;
                                }
                            }
                            None => {}
                        }
                    }
                };

                if let Some(ref mut fan_widget) = self.show_fan {
                    if controlled_changed {
                        if self.fan_speed_controlled {
                            Command::new("nvidia-settings")
                                .args(&[
                                    "-a",
                                    &format!("[gpu:{}]/GPUFanControlState=1", self.gpu_id),
                                    "-a",
                                    &format!(
                                        "[fan:{}]/GPUTargetFanSpeed={}",
                                        self.gpu_id, self.fan_speed
                                    ),
                                ])
                                .output()
                                .block_error("gpu", "Failed to execute nvidia-settings.")?;
                            fan_widget.set_text(format!("{:02}%", self.fan_speed));
                            fan_widget.set_state(State::Warning);
                        } else {
                            Command::new("nvidia-settings")
                                .args(&[
                                    "-a",
                                    &format!("[gpu:{}]/GPUFanControlState=0", self.gpu_id),
                                ])
                                .output()
                                .block_error("gpu", "Failed to execute nvidia-settings.")?;
                            fan_widget.set_state(State::Idle);
                        }
                    } else if self.fan_speed_controlled {
                        Command::new("nvidia-settings")
                            .args(&[
                                "-a",
                                &format!(
                                    "[fan:{}]/GPUTargetFanSpeed={}",
                                    self.gpu_id, new_fan_speed
                                ),
                            ])
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

    fn id(&self) -> usize {
        self.id
    }
}

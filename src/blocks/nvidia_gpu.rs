use std::process::Command;
use std::time::Duration;

use crossbeam_channel::Sender;
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::SharedConfig;
use crate::config::{LogicalDirection, Scrolling};
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::protocol::i3bar_event::{I3BarEvent, MouseButton};
use crate::scheduler::Task;
use crate::util::pseudo_uuid;
use crate::widgets::text::TextWidget;
use crate::widgets::{I3BarWidget, Spacing, State};

pub struct NvidiaGpu {
    id: usize,
    id_fans: usize,
    id_memory: usize,
    update_interval: Duration,

    gpu_enabled: bool,
    gpu_id: u64,

    name_widget: TextWidget,
    name_widget_mode: NameWidgetMode,
    label: String,

    show_memory: Option<TextWidget>,
    memory_widget_mode: MemoryWidgetMode,

    show_utilization: Option<TextWidget>,
    show_temperature: Option<TextWidget>,

    show_fan: Option<TextWidget>,
    fan_speed: u64,
    fan_speed_controlled: bool,
    scrolling: Scrolling,

    show_clocks: Option<TextWidget>,

    show_power_draw: Option<TextWidget>,

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

// TODO add `format` option
#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct NvidiaGpuConfig {
    /// Update interval in seconds
    #[serde(deserialize_with = "deserialize_duration")]
    pub interval: Duration,

    /// Label to show instead of the default GPU name from `nvidia-smi`
    pub label: Option<String>,

    /// GPU id in system
    pub gpu_id: u64,

    /// GPU utilization. In percent.
    pub show_utilization: bool,

    /// VRAM utilization.
    pub show_memory: bool,

    /// Core GPU temperature. In degrees C.
    pub show_temperature: bool,

    /// Fan speed. In percents.
    pub show_fan_speed: bool,

    /// GPU clocks. In percents.
    pub show_clocks: bool,

    /// Last Measured Power Draw of GPU. In Watts.
    pub show_power_draw: bool,

    /// Maximum temperature, below which state is set to idle
    pub idle: u64,

    /// Maximum temperature, below which state is set to good
    pub good: u64,

    /// Maximum temperature, below which state is set to info
    pub info: u64,

    /// Maximum temperature, below which state is set to warning
    pub warning: u64,
}

impl Default for NvidiaGpuConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(3),
            label: None,
            gpu_id: 0,
            show_utilization: true,
            show_memory: true,
            show_temperature: true,
            show_fan_speed: false,
            show_clocks: false,
            show_power_draw: false,
            idle: 50,
            good: 70,
            info: 75,
            warning: 80,
        }
    }
}

impl ConfigBlock for NvidiaGpu {
    type Config = NvidiaGpuConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        shared_config: SharedConfig,
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

            name_widget: TextWidget::new(id, id, shared_config.clone())
                .with_icon("gpu")?
                .with_spacing(Spacing::Inline),
            name_widget_mode: if block_config.label.is_some() {
                NameWidgetMode::ShowLabel
            } else {
                NameWidgetMode::ShowDefaultName
            },
            label: block_config.label.unwrap_or_default(),

            show_memory: if block_config.show_memory {
                Some(
                    TextWidget::new(id, id_memory, shared_config.clone())
                        .with_spacing(Spacing::Inline),
                )
            } else {
                None
            },
            memory_widget_mode: MemoryWidgetMode::ShowUsedMemory,

            show_utilization: if block_config.show_utilization {
                Some(TextWidget::new(id, id, shared_config.clone()).with_spacing(Spacing::Inline))
            } else {
                None
            },

            show_temperature: if block_config.show_temperature {
                Some(TextWidget::new(id, id, shared_config.clone()).with_spacing(Spacing::Inline))
            } else {
                None
            },

            show_fan: if block_config.show_fan_speed {
                Some(
                    TextWidget::new(id, id_fans, shared_config.clone())
                        .with_spacing(Spacing::Inline),
                )
            } else {
                None
            },
            fan_speed: 0,
            fan_speed_controlled: false,
            scrolling: shared_config.scrolling,

            show_clocks: if block_config.show_clocks {
                Some(TextWidget::new(id, id, shared_config.clone()).with_spacing(Spacing::Inline))
            } else {
                None
            },

            show_power_draw: if block_config.show_power_draw {
                Some(TextWidget::new(id, id, shared_config).with_spacing(Spacing::Inline))
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
        if self.show_power_draw.is_some() {
            params += "power.draw,";
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
                count += 1;
            }
            if let Some(ref mut power_draw_widget) = self.show_power_draw {
                power_draw_widget.set_text(format!("{} W", result[count]));
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
            if let Some(ref power_draw_widget) = self.show_power_draw {
                widgets.push(power_draw_widget);
            }
        }
        widgets
    }

    fn click(&mut self, e: &I3BarEvent) -> Result<()> {
        if let Some(event_id) = e.instance {
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

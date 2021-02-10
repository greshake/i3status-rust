use std::collections::{BTreeMap, HashMap};
use std::process::Command;
use std::time::Duration;

use crossbeam_channel::Sender;
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::Config;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::input::{I3BarEvent, MouseButton};
use crate::scheduler::Task;
use crate::util::FormatTemplate;
use crate::widget::{I3BarWidget, Spacing, State};
use crate::widgets::button::ButtonWidget;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TemperatureScale {
    Celsius,
    Fahrenheit,
}

impl Default for TemperatureScale {
    fn default() -> Self {
        Self::Celsius
    }
}

pub struct Temperature {
    id: usize,
    text: ButtonWidget,
    output: String,
    collapsed: bool,
    update_interval: Duration,
    scale: TemperatureScale,
    maximum_good: i64,
    maximum_idle: i64,
    maximum_info: i64,
    maximum_warning: i64,
    format: FormatTemplate,
    chip: Option<String>,
    inputs: Option<Vec<String>>,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct TemperatureConfig {
    /// Update interval in seconds
    #[serde(
        default = "TemperatureConfig::default_interval",
        deserialize_with = "deserialize_duration"
    )]
    pub interval: Duration,

    /// Collapsed by default?
    #[serde(default = "TemperatureConfig::default_collapsed")]
    pub collapsed: bool,

    /// The temperature scale to use for display and thresholds
    #[serde(default)]
    pub scale: TemperatureScale,

    /// Maximum temperature, below which state is set to good
    #[serde(default)]
    pub good: Option<i64>,

    /// Maximum temperature, below which state is set to idle
    #[serde(default)]
    pub idle: Option<i64>,

    /// Maximum temperature, below which state is set to info
    #[serde(default)]
    pub info: Option<i64>,

    /// Maximum temperature, below which state is set to warning
    #[serde(default)]
    pub warning: Option<i64>,

    /// Format override
    #[serde(default = "TemperatureConfig::default_format")]
    pub format: String,

    /// Chip override
    #[serde(default = "TemperatureConfig::default_chip")]
    pub chip: Option<String>,

    /// Inputs whitelist
    #[serde(default = "TemperatureConfig::default_inputs")]
    pub inputs: Option<Vec<String>>,

    #[serde(default = "TemperatureConfig::default_color_overrides")]
    pub color_overrides: Option<BTreeMap<String, String>>,
}

impl TemperatureConfig {
    fn default_format() -> String {
        "{average}° avg, {max}° max".to_owned()
    }

    fn default_interval() -> Duration {
        Duration::from_secs(5)
    }

    fn default_collapsed() -> bool {
        true
    }

    fn default_chip() -> Option<String> {
        None
    }

    fn default_inputs() -> Option<Vec<String>> {
        None
    }

    fn default_color_overrides() -> Option<BTreeMap<String, String>> {
        None
    }
}

impl ConfigBlock for Temperature {
    type Config = TemperatureConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        config: Config,
        _tx_update_request: Sender<Task>,
    ) -> Result<Self> {
        Ok(Temperature {
            id,
            update_interval: block_config.interval,
            text: ButtonWidget::new(config, id)
                .with_icon("thermometer")
                .with_spacing(if block_config.collapsed {
                    Spacing::Hidden
                } else {
                    Spacing::Normal
                }),
            output: String::new(),
            collapsed: block_config.collapsed,
            scale: block_config.scale,
            maximum_good: block_config
                .good
                .unwrap_or_else(|| match block_config.scale {
                    TemperatureScale::Celsius => 20,
                    TemperatureScale::Fahrenheit => 68,
                }),
            maximum_idle: block_config
                .idle
                .unwrap_or_else(|| match block_config.scale {
                    TemperatureScale::Celsius => 45,
                    TemperatureScale::Fahrenheit => 113,
                }),
            maximum_info: block_config
                .info
                .unwrap_or_else(|| match block_config.scale {
                    TemperatureScale::Celsius => 60,
                    TemperatureScale::Fahrenheit => 140,
                }),
            maximum_warning: block_config
                .warning
                .unwrap_or_else(|| match block_config.scale {
                    TemperatureScale::Celsius => 80,
                    TemperatureScale::Fahrenheit => 176,
                }),
            format: FormatTemplate::from_string(&block_config.format)
                .block_error("temperature", "Invalid format specified for temperature")?,
            chip: block_config.chip,
            inputs: block_config.inputs,
        })
    }
}

type SensorsOutput = HashMap<String, HashMap<String, serde_json::Value>>;
type InputReadings = HashMap<String, f64>;

impl Block for Temperature {
    fn update(&mut self) -> Result<Option<Update>> {
        let mut args = vec!["-j"];
        if let TemperatureScale::Fahrenheit = self.scale {
            args.push("-f");
        }
        if let Some(ref chip) = &self.chip {
            args.push(chip);
        }
        let output = Command::new("sensors")
            .args(&args)
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
            .unwrap_or_else(|e| e.to_string());

        let parsed: SensorsOutput = serde_json::from_str(&output)
            .block_error("temperature", "sensors output is invalid")?;

        let mut temperatures: Vec<i64> = Vec::new();
        for (_chip, inputs) in parsed {
            for (input_name, input_values) in inputs {
                if let Some(ref whitelist) = self.inputs {
                    if !whitelist.contains(&input_name) {
                        continue;
                    }
                }

                let values_parsed: InputReadings = match serde_json::from_value(input_values) {
                    Ok(values) => values,
                    Err(_) => continue, // probably the "Adapter" key, just ignore.
                };

                for (value_name, value) in values_parsed {
                    if !value_name.starts_with("temp") || !value_name.ends_with("input") {
                        continue;
                    }

                    if value > -101f64 && value < 151f64 {
                        temperatures.push(value as i64);
                    } else {
                        // This error is recoverable and therefore should not stop the program
                        eprintln!("Temperature ({}) outside of range ([-100, 150])", value);
                    }
                }
            }
        }

        if !temperatures.is_empty() {
            let max: i64 = *temperatures
                .iter()
                .max()
                .block_error("temperature", "failed to get max temperature")?;
            let min: i64 = *temperatures
                .iter()
                .min()
                .block_error("temperature", "failed to get min temperature")?;
            let avg: i64 = (temperatures.iter().sum::<i64>() as f64 / temperatures.len() as f64)
                .round() as i64;

            let values = map!("{average}" => avg,
                              "{min}" => min,
                              "{max}" => max);

            self.output = self.format.render_static_str(&values)?;
            if !self.collapsed {
                self.text.set_text(self.output.clone());
            }

            let state = match max {
                m if m <= self.maximum_good => State::Good,
                m if m <= self.maximum_idle => State::Idle,
                m if m <= self.maximum_info => State::Info,
                m if m <= self.maximum_warning => State::Warning,
                _ => State::Critical,
            };

            self.text.set_state(state);
        }

        Ok(Some(self.update_interval.into()))
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.text]
    }

    fn click(&mut self, e: &I3BarEvent) -> Result<()> {
        if e.matches_id(self.id) && e.button == MouseButton::Left {
            self.collapsed = !self.collapsed;
            if self.collapsed {
                self.text.set_text(String::new());
                self.text.set_spacing(Spacing::Hidden);
            } else {
                self.text.set_text(self.output.clone());
                self.text.set_spacing(Spacing::Normal);
            }
        }

        Ok(())
    }

    fn id(&self) -> usize {
        self.id
    }
}

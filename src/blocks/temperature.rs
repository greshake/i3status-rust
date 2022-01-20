use std::fs;
use std::time::Duration;

use sensors::FeatureType::SENSORS_FEATURE_TEMP;
use sensors::Sensors;
use sensors::SubfeatureType::SENSORS_SUBFEATURE_TEMP_INPUT;

use crossbeam_channel::Sender;
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::SharedConfig;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::formatting::value::Value;
use crate::formatting::FormatTemplate;
use crate::protocol::i3bar_event::{I3BarEvent, MouseButton};
use crate::scheduler::Task;
use crate::widgets::{text::TextWidget, I3BarWidget, Spacing, State};

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

#[derive(Copy, Clone, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TemperatureDriver {
    Sysfs,
    Sensors,
}

impl Default for TemperatureDriver {
    fn default() -> Self {
        TemperatureDriver::Sensors
    }
}

pub struct Temperature {
    id: usize,
    text: TextWidget,
    output: (String, Option<String>),
    collapsed: bool,
    update_interval: Duration,
    scale: TemperatureScale,
    maximum_good: f64,
    maximum_idle: f64,
    maximum_info: f64,
    maximum_warning: f64,
    format: FormatTemplate,
    // DEPRECATED
    driver: TemperatureDriver,
    chip: Option<String>,
    inputs: Option<Vec<String>>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct TemperatureConfig {
    /// Update interval in seconds
    #[serde(deserialize_with = "deserialize_duration")]
    pub interval: Duration,

    /// Collapsed by default?
    pub collapsed: bool,

    /// The temperature scale to use for display and thresholds
    #[serde(default)]
    pub scale: TemperatureScale,

    /// Maximum temperature, below which state is set to good
    #[serde(default)]
    pub good: Option<f64>,

    /// Maximum temperature, below which state is set to idle
    #[serde(default)]
    pub idle: Option<f64>,

    /// Maximum temperature, below which state is set to info
    #[serde(default)]
    pub info: Option<f64>,

    /// Maximum temperature, below which state is set to warning
    #[serde(default)]
    pub warning: Option<f64>,

    /// Format override
    pub format: FormatTemplate,

    /// The "driver " to use for temperature block. One of "sysfs" or "sensors"
    pub driver: TemperatureDriver,

    /// Chip override
    pub chip: Option<String>,

    /// Inputs whitelist
    pub inputs: Option<Vec<String>>,
}

impl Default for TemperatureConfig {
    fn default() -> Self {
        Self {
            format: FormatTemplate::default(),
            interval: Duration::from_secs(5),
            collapsed: true,
            scale: TemperatureScale::default(),
            good: None,
            idle: None,
            info: None,
            warning: None,
            driver: TemperatureDriver::default(),
            chip: None,
            inputs: None,
        }
    }
}

impl ConfigBlock for Temperature {
    type Config = TemperatureConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        shared_config: SharedConfig,
        _tx_update_request: Sender<Task>,
    ) -> Result<Self> {
        Ok(Temperature {
            id,
            update_interval: block_config.interval,
            text: TextWidget::new(id, 0, shared_config)
                .with_icon("thermometer")?
                .with_spacing(if block_config.collapsed {
                    Spacing::Hidden
                } else {
                    Spacing::Normal
                }),
            output: (String::new(), None),
            collapsed: block_config.collapsed,
            scale: block_config.scale,
            maximum_good: block_config.good.unwrap_or(match block_config.scale {
                TemperatureScale::Celsius => 20f64,
                TemperatureScale::Fahrenheit => 68f64,
            }),
            maximum_idle: block_config.idle.unwrap_or(match block_config.scale {
                TemperatureScale::Celsius => 45f64,
                TemperatureScale::Fahrenheit => 113f64,
            }),
            maximum_info: block_config.info.unwrap_or(match block_config.scale {
                TemperatureScale::Celsius => 60f64,
                TemperatureScale::Fahrenheit => 140f64,
            }),
            maximum_warning: block_config.warning.unwrap_or(match block_config.scale {
                TemperatureScale::Celsius => 80f64,
                TemperatureScale::Fahrenheit => 176f64,
            }),
            format: block_config
                .format
                .with_default("{average} avg, {max} max")?,
            driver: block_config.driver,
            chip: block_config.chip,
            inputs: block_config.inputs,
        })
    }
}

impl Block for Temperature {
    fn update(&mut self) -> Result<Option<Update>> {
        let mut temperatures: Vec<f64> = Vec::new();

        match self.driver {
            TemperatureDriver::Sensors => {
                let sensors = Sensors::new();

                let chips = match &self.chip {
                    Some(chip) => sensors
                        .detected_chips(chip)
                        .block_error("temperature", "Failed to create chip iterator")?,
                    None => sensors.into_iter(),
                };

                for chip in chips {
                    for feat in chip {
                        if *feat.feature_type() != SENSORS_FEATURE_TEMP {
                            continue;
                        }
                        if let Some(inputs) = &self.inputs {
                            let label = feat
                                .get_label()
                                .block_error("temperature", "Failed to get input label")?;
                            if !inputs.contains(&label) {
                                continue;
                            }
                        }
                        for subfeat in feat {
                            if *subfeat.subfeature_type() == SENSORS_SUBFEATURE_TEMP_INPUT {
                                if let Ok(value) = subfeat.get_value() {
                                    if (-100.0..150.0).contains(&value) {
                                        temperatures.push(value);
                                    } else {
                                        eprintln!(
                                            "Temperature ({}) outside of range ([-100, 150])",
                                            value
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
            TemperatureDriver::Sysfs => {
                for hwmon_dir in fs::read_dir("/sys/class/hwmon")? {
                    let hwmon = &hwmon_dir?.path();
                    if let Some(ref chip_name) = self.chip {
                        // Narrow to hwmon names that are substrings of the given chip name or vice versa
                        let hwmon_untrimmed = fs::read_to_string(hwmon.join("name"))?;
                        let hwmon_name = hwmon_untrimmed.trim();
                        if !(chip_name.contains(hwmon_name) || hwmon_name.contains(chip_name)) {
                            continue;
                        }
                    }
                    for entry in hwmon.read_dir()? {
                        let entry = entry?;
                        if let Ok(name) = entry.file_name().into_string() {
                            if name.starts_with("temp") && name.ends_with("label") {
                                if let Some(ref whitelist) = self.inputs {
                                    //narrow to labels that are an exact match for one of the inputs
                                    if !whitelist.contains(
                                        &fs::read_to_string(entry.path())?.trim().to_string(),
                                    ) {
                                        continue;
                                    }
                                }
                                let value: f64 =
                                    fs::read_to_string(hwmon.join(name.replace("label", "input")))?
                                        .trim()
                                        .parse::<f64>()
                                        .block_error(
                                            "temperature",
                                            "failed to parse temperature as an integer",
                                        )?
                                        / 1000f64;

                                if value > -101f64 && value < 151f64 {
                                    temperatures.push(value);
                                } else {
                                    // This error is recoverable and therefore should not stop the program
                                    eprintln!(
                                        "Temperature ({}) outside of range ([-100, 150])",
                                        value
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }

        if let TemperatureScale::Fahrenheit = self.scale {
            temperatures
                .iter_mut()
                .for_each(|c| *c = *c * 9f64 / 5f64 + 32f64);
        }

        if !temperatures.is_empty() {
            let max: f64 = temperatures
                .iter()
                .cloned()
                .reduce(f64::max)
                .block_error("temperature", "failed to get max temperature")?;
            let min: f64 = temperatures
                .iter()
                .cloned()
                .reduce(f64::min)
                .block_error("temperature", "failed to get min temperature")?;
            let avg: f64 = temperatures.iter().sum::<f64>() / temperatures.len() as f64;

            let values = map!(
                "average" => Value::from_float(avg).degrees(),
                "min" => Value::from_float(min).degrees(),
                "max" => Value::from_float(max).degrees()
            );

            self.output = self.format.render(&values)?;
            if !self.collapsed {
                self.text.set_texts(self.output.clone());
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
        if e.button == MouseButton::Left {
            self.collapsed = !self.collapsed;
            if self.collapsed {
                self.text.set_text(String::new());
            } else {
                self.text.set_texts(self.output.clone());
            }
        }

        Ok(())
    }

    fn id(&self) -> usize {
        self.id
    }
}

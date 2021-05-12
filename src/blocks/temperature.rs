use std::collections::HashMap;
use std::process::Command;
use std::time::Duration;

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
use crate::util::has_command;
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

pub struct Temperature {
    id: usize,
    text: TextWidget,
    output: (String, Option<String>),
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
    fallback_required: bool,
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
    pub format: FormatTemplate,

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
            format: block_config
                .format
                .with_default("{average} avg, {max} max")?,
            chip: block_config.chip,
            inputs: block_config.inputs,
            fallback_required: !has_command("temperature", "sensors -j").unwrap_or(false),
        })
    }
}

type SensorsOutput = HashMap<String, HashMap<String, serde_json::Value>>;
type InputReadings = HashMap<String, f64>;

impl Block for Temperature {
    fn update(&mut self) -> Result<Option<Update>> {
        let mut args = if self.fallback_required {
            vec!["-u"]
        } else {
            vec!["-j"]
        };

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

        let mut temperatures: Vec<i64> = Vec::new();

        if self.fallback_required {
            for line in output.lines() {
                if let Some(rest) = line.strip_prefix("  temp") {
                    let rest = rest
                        .split('_')
                        .flat_map(|x| x.split(' '))
                        .flat_map(|x| x.split('.'))
                        .collect::<Vec<_>>();

                    if rest[1].starts_with("input") {
                        match rest[2].parse::<i64>() {
                            Ok(t) if t == 0 => Ok(()),
                            Ok(t) if t > -101 && t < 151 => {
                                temperatures.push(t);
                                Ok(())
                            }
                            Ok(t) => {
                                // This error is recoverable and therefore should not stop the program
                                eprintln!("Temperature ({}) outside of range ([-100, 150])", t);
                                Ok(())
                            }
                            Err(_) => Err(BlockError(
                                "temperature".to_owned(),
                                "failed to parse temperature as an integer".to_owned(),
                            )),
                        }?
                    }
                }
            }
        } else {
            let parsed: SensorsOutput = serde_json::from_str(&output)
                .block_error("temperature", "sensors output is invalid")?;
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

            let values = map!(
                "average" => Value::from_integer(avg).degrees(),
                "min" => Value::from_integer(min).degrees(),
                "max" => Value::from_integer(max).degrees()
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
                self.text.set_spacing(Spacing::Hidden);
            } else {
                self.text.set_texts(self.output.clone());
                self.text.set_spacing(Spacing::Normal);
            }
        }

        Ok(())
    }

    fn id(&self) -> usize {
        self.id
    }
}

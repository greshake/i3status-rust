//! The system temperature
//!
//! This block displays the system temperature, based on `libsensors` library.
//!
//! This block has two modes: "collapsed", which uses only color as an indicator, and "expanded",
//! which shows the content of a `format` string. The average, minimum, and maximum temperatures
//! are computed using all sensors displayed by `sensors`, or optionally filtered by `chip` and
//! `inputs`.
//!
//! Requires `libsensors` and appropriate kernel modules for your hardware.
//!
//! Run `sensors` command to list available chips and inputs.
//!
//! Note that the colour of the block is always determined by the maximum temperature across all
//! sensors, not the average. You may need to keep this in mind if you have a misbehaving sensor.
//!
//! # Configuration
//!
//! Key | Values | Required | Default
//! ----|--------|----------|--------
//! `format` | A string to customise the output of this block. See below for available placeholders | No | `"$average avg, $max max|"`
//! `interval` | Update interval in seconds | No | `5`
//! `collapsed` | Whether the block will be collapsed by default | No | `false`
//! `scale` | Either `"celsius"` or `"fahrenheit"` | No | `"celsius"`
//! `good` | Maximum temperature to set state to good | No | `20` °C (`68` °F)
//! `idle` | Maximum temperature to set state to idle | No | `45` °C (`113` °F)
//! `info` | Maximum temperature to set state to info | No | `60` °C (`140` °F)
//! `warning` | Maximum temperature to set state to warning. Beyond this temperature, state is set to critical | No | `80` °C (`176` °F)
//! `chip` | Narrows the results to a given chip name. `*` may be used as a wildcard. | No | None
//! `inputs` | Narrows the results to individual inputs reported by each chip. | No | None
//!
//! Placeholder | Value                                | Type   | Unit
//! ------------|--------------------------------------|--------|--------
//! `min`       | Minimum temperature among all inputs | Number | Degrees
//! `average`   | Average temperature among all inputs | Number | Degrees
//! `max`       | Maximum temperature among all inputs | Number | Degrees
//!
//! Note that when block is collapsed, no placeholders are provided.
//!
//! # Example
//!
//! ```toml
//! [[block]]
//! block = "temperature"
//! format = "{min} min, {max} max, {average} avg|"
//! interval = 10
//! chip = "*-isa-*"
//! ```
//!
//! # Icons Used
//! - `thermometer`

use std::collections::HashMap;

use super::prelude::*;

use sensors::FeatureType::SENSORS_FEATURE_TEMP;
use sensors::Sensors;
use sensors::SubfeatureType::SENSORS_SUBFEATURE_TEMP_INPUT;

const DEFAULT_GOOD: f64 = 20.0;
const DEFAULT_IDLE: f64 = 45.0;
const DEFAULT_INFO: f64 = 60.0;
const DEFAULT_WARN: f64 = 80.0;

#[derive(Deserialize, Debug, Derivative)]
#[serde(deny_unknown_fields, default)]
#[derivative(Default)]
struct TemperatureConfig {
    format: FormatConfig,
    #[derivative(Default(value = "5.into()"))]
    interval: Seconds,
    collapsed: bool,
    scale: TemperatureScale,
    good: Option<f64>,
    idle: Option<f64>,
    info: Option<f64>,
    warning: Option<f64>,
    chip: Option<String>,
    inputs: Option<Vec<StdString>>,
}

#[derive(Deserialize, Debug, Derivative, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[derivative(Default)]
enum TemperatureScale {
    #[derivative(Default)]
    Celsius,
    Fahrenheit,
}

impl TemperatureScale {
    #[allow(clippy::wrong_self_convention)]
    pub fn from_celsius(self, val: f64) -> f64 {
        match self {
            Self::Celsius => val,
            Self::Fahrenheit => val * 1.8 + 32.0,
        }
    }
}

pub async fn run(config: toml::Value, mut api: CommonApi) -> Result<()> {
    let mut events = api.get_events().await?;
    let config = TemperatureConfig::deserialize(config).config_error()?;
    let mut collapsed = config.collapsed;
    api.set_format(config.format.with_default("$average avg, $max max")?);
    api.set_icon("thermometer")?;

    let good = config
        .good
        .unwrap_or_else(|| config.scale.from_celsius(DEFAULT_GOOD));
    let idle = config
        .idle
        .unwrap_or_else(|| config.scale.from_celsius(DEFAULT_IDLE));
    let info = config
        .info
        .unwrap_or_else(|| config.scale.from_celsius(DEFAULT_INFO));
    let warn = config
        .warning
        .unwrap_or_else(|| config.scale.from_celsius(DEFAULT_WARN));

    loop {
        // Perhaps it's better to just Box::leak() once and don't clone() every time?
        let chip = config.chip.clone();
        let inputs = config.inputs.clone();
        let temp = tokio::task::spawn_blocking(move || {
            let mut vals = Vec::new();
            let sensors = Sensors::new();
            let chips = match &chip {
                Some(chip) => sensors
                    .detected_chips(chip)
                    .error("Failed to create chip iterator")?,
                None => sensors.into_iter(),
            };
            for chip in chips {
                for feat in chip {
                    if *feat.feature_type() != SENSORS_FEATURE_TEMP {
                        continue;
                    }
                    if let Some(inputs) = &inputs {
                        let label = feat.get_label().error("Failed to get input label")?;
                        if !inputs.contains(&label) {
                            continue;
                        }
                    }
                    for subfeat in feat {
                        if *subfeat.subfeature_type() == SENSORS_SUBFEATURE_TEMP_INPUT {
                            if let Ok(value) = subfeat.get_value() {
                                if (-100.0..=150.0).contains(&value) {
                                    vals.push(config.scale.from_celsius(value));
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
            Ok(vals)
        })
        .await
        .error("Failed to join tokio task")??;

        let min_temp = temp
            .iter()
            .min_by(|a, b| a.partial_cmp(b).unwrap())
            .cloned()
            .unwrap_or(0.0);
        let max_temp = temp
            .iter()
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .cloned()
            .unwrap_or(0.0);
        let avg_temp = temp.iter().sum::<f64>() / temp.len() as f64;

        api.set_state(match max_temp {
            x if x <= good => State::Good,
            x if x <= idle => State::Idle,
            x if x <= info => State::Info,
            x if x <= warn => State::Warning,
            _ => State::Critical,
        });

        'outer: loop {
            if collapsed {
                api.set_values(HashMap::new());
            } else {
                api.set_values(map! {
                    "average" => Value::degrees(avg_temp),
                    "min" => Value::degrees(min_temp),
                    "max" => Value::degrees(max_temp),
                });
            }

            api.flush().await?;

            loop {
                tokio::select! {
                    _ = sleep(config.interval.0) => break 'outer,
                    Some(BlockEvent::Click(click)) = events.recv() => {
                        if click.button == MouseButton::Left  {
                            collapsed = !collapsed;
                            break;
                        }
                    }
                }
            }
        }
    }
}

/*
#[derive(Debug, Clone)]
struct ChipInfo {
    temp: Vec<i32>,
}

impl ChipInfo {
    async fn new(name: &str) -> Result<Self> {
        let mut sysfs_dir = read_dir("/sys/class/hwmon")
            .await
            .error("failed to read /sys/class/hwmon direcory")?;
        while let Some(dir) = sysfs_dir
            .next_entry()
            .await
            .error("failed to read /sys/class/hwmon direcory")?
        {
            if read_to_string(dir.path().join("name"))
                .await
                .map(|t| t.trim() == name)
                .unwrap_or(false)
            {
                let mut chip_dir = read_dir(dir.path())
                    .await
                    .error("failed to read chip's sysfs direcory")?;
                let mut temp = Vec::new();
                while let Some(entry) = chip_dir
                    .next_entry()
                    .await
                    .error("failed to read chip's sysfs direcory")?
                {
                    let entry_str = entry.file_name().to_str().unwrap().to_string();
                    if entry_str.starts_with("temp") && entry_str.ends_with("_input") {
                        let val: i32 = read_to_string(entry.path())
                            .await
                            .error("failed to read chip's temperature")?
                            .trim()
                            .parse()
                            .error("temperature is not an integer")?;
                        temp.push(val / 1000);
                    }
                }
                return Ok(Self { temp });
            }
        }
        Err(Error::new(format!("chip '{}' not found", name)))
    }
}
*/

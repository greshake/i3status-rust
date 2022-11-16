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
//! Key | Values | Default
//! ----|--------|--------
//! `format` | A string to customise the output of this block. See below for available placeholders | `" $icon $average avg, $max max "`
//! `format_alt` | If set, block will switch between `format` and `format_alt` on every click | `None`
//! `interval` | Update interval in seconds | `5`
//! `scale` | Either `"celsius"` or `"fahrenheit"` | `"celsius"`
//! `good` | Maximum temperature to set state to good | `20` °C (`68` °F)
//! `idle` | Maximum temperature to set state to idle | `45` °C (`113` °F)
//! `info` | Maximum temperature to set state to info | `60` °C (`140` °F)
//! `warning` | Maximum temperature to set state to warning. Beyond this temperature, state is set to critical | `80` °C (`176` °F)
//! `chip` | Narrows the results to a given chip name. `*` may be used as a wildcard. | None
//! `inputs` | Narrows the results to individual inputs reported by each chip. | None
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
//! format = " $icon $max max "
//! format_alt = " $icon $min min, $max max, $average avg "
//! interval = 10
//! chip = "*-isa-*"
//! ```
//!
//! # Icons Used
//! - `thermometer`

use super::prelude::*;
use sensors::FeatureType::SENSORS_FEATURE_TEMP;
use sensors::Sensors;
use sensors::SubfeatureType::SENSORS_SUBFEATURE_TEMP_INPUT;

const DEFAULT_GOOD: f64 = 20.0;
const DEFAULT_IDLE: f64 = 45.0;
const DEFAULT_INFO: f64 = 60.0;
const DEFAULT_WARN: f64 = 80.0;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(default)]
pub struct Config {
    format: FormatConfig,
    format_alt: Option<FormatConfig>,
    #[default(5.into())]
    interval: Seconds,
    scale: TemperatureScale,
    good: Option<f64>,
    idle: Option<f64>,
    info: Option<f64>,
    warning: Option<f64>,
    chip: Option<String>,
    inputs: Option<Vec<String>>,
}

#[derive(Deserialize, Debug, SmartDefault, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum TemperatureScale {
    #[default]
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

pub async fn run(config: Config, mut api: CommonApi) -> Result<()> {
    let mut format = config
        .format
        .with_default(" $icon $average avg, $max max ")?;
    let mut format_alt = match config.format_alt {
        Some(f) => Some(f.with_default("")?),
        None => None,
    };
    let mut widget = Widget::new().with_format(format.clone());

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

        widget.state = match max_temp {
            x if x <= good => State::Good,
            x if x <= idle => State::Idle,
            x if x <= info => State::Info,
            x if x <= warn => State::Warning,
            _ => State::Critical,
        };

        'outer: loop {
            widget.set_values(map! {
                "icon" => Value::icon(api.get_icon("thermometer")?),
                "average" => Value::degrees(avg_temp),
                "min" => Value::degrees(min_temp),
                "max" => Value::degrees(max_temp),
            });

            api.set_widget(&widget).await?;

            loop {
                select! {
                    _ = sleep(config.interval.0) => break 'outer,
                    event = api.event() => match event {
                        UpdateRequest => break 'outer,
                        Click(click) => {
                            if click.button == MouseButton::Left  {
                                if let Some(ref mut format_alt) = format_alt {
                                    std::mem::swap(format_alt, &mut format);
                                    widget.set_format(format.clone());
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

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
//! `format` | A [MultiFormat][MaybeMultiFormatConfig] string to customise the output of this block. See below for available placeholders | `[" $icon $average avg, $max max "]`
//! `interval` | Update interval in seconds | `5`
//! `scale` | Either `"celsius"` or `"fahrenheit"` | `"celsius"`
//! `good` | Maximum temperature to set state to good | `20` °C (`68` °F)
//! `idle` | Maximum temperature to set state to idle | `45` °C (`113` °F)
//! `info` | Maximum temperature to set state to info | `60` °C (`140` °F)
//! `warning` | Maximum temperature to set state to warning. Beyond this temperature, state is set to critical | `80` °C (`176` °F)
//! `chip` | Narrows the results to a given chip name. `*` may be used as a wildcard. | None
//! `inputs` | Narrows the results to individual inputs reported by each chip. | None
//!
//! Action          | Description                               | Default button
//! ----------------|-------------------------------------------|---------------
//! `toggle_format` **DEPRECATED** | Toggles between `format` and `format_alt` | -
//! `next_format`  | Switches to the next format in the list     | Left
//! `prev_format`  | Switches to the previous format in the list | Right
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
use crate::util::celsius_to_fahrenheit;
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
    #[serde(flatten)]
    pub formats: MaybeMultiFormatConfig,
    #[default(5.into())]
    pub interval: Seconds,
    pub scale: TemperatureScale,
    pub good: Option<f64>,
    pub idle: Option<f64>,
    pub info: Option<f64>,
    pub warning: Option<f64>,
    pub chip: Option<String>,
    pub inputs: Option<Vec<String>>,
}

#[derive(Deserialize, Debug, SmartDefault, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TemperatureScale {
    #[default]
    Celsius,
    Fahrenheit,
}

impl TemperatureScale {
    #[allow(clippy::wrong_self_convention)]
    pub fn from_celsius(self, val: f64) -> f64 {
        match self {
            Self::Celsius => val,
            Self::Fahrenheit => celsius_to_fahrenheit(val),
        }
    }

    pub fn as_value(self, val: f64) -> Value {
        match self {
            Self::Celsius => Value::degrees_c(val),
            Self::Fahrenheit => Value::degrees_f(val),
        }
    }
}

pub(crate) fn prepare(config: &Config) -> Result<Arc<BlockPlan>> {
    let declare = |output: OutputPlan| {
        output
            .icon("icon", IconChoices::one("thermometer"))
            .always_provides("icon", ValueKind::Icon)
            .always_provides("average", ValueKind::Number)
            .always_provides("min", ValueKind::Number)
            .always_provides("max", ValueKind::Number)
    };
    let mut outputs = vec![declare(OutputPlan::new(
        "main",
        config
            .format
            .with_default(" $icon $average avg, $max max ")?,
    ))];
    if let Some(format_alt) = &config.format_alt {
        outputs.push(declare(OutputPlan::new(
            "alt",
            format_alt.with_default("")?,
        )));
    }
    Ok(BlockPlan::new(outputs))
}

pub async fn run(config: &Config, api: &CommonApi, plan: &Arc<BlockPlan>) -> Result<()> {
    let mut actions = api.get_actions()?;
    api.set_default_actions(&[
        (MouseButton::Left, None, "next_format"),
        (MouseButton::Right, None, "prev_format"),
    ])?;

    let output_main = plan.output("main")?;
    let output_alt = match &config.format_alt {
        Some(_) => Some(plan.output("alt")?),
        None => None,
    };
    let mut alt_shown = false;

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
        let chip = config.chip.clone();
        let inputs = config.inputs.clone();
        let config_scale = config.scale;
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
                        if *subfeat.subfeature_type() == SENSORS_SUBFEATURE_TEMP_INPUT
                            && let Ok(value) = subfeat.get_value()
                        {
                            if (-100.0..=150.0).contains(&value) {
                                vals.push(config_scale.from_celsius(value));
                            } else {
                                eprintln!("Temperature ({value}) outside of range ([-100, 150])");
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

        let output = match (&output_alt, alt_shown) {
            (Some(alt), true) => alt,
            _ => &output_main,
        };
        let mut widget = output.new_widget();

        widget.state = match max_temp {
            x if x <= good => State::Good,
            x if x <= idle => State::Idle,
            x if x <= info => State::Info,
            x if x <= warn => State::Warning,
            _ => State::Critical,
        };

        widget.set_values(map! {
            "icon" => output.icon_progression_bound("icon", "thermometer", max_temp, good, warn)?,
            "average" => config_scale.as_value(avg_temp),
            "min" => config_scale.as_value(min_temp),
            "max" => config_scale.as_value(max_temp),
        });

        api.set_widget(widget)?;

        select! {
            _ = sleep(config.interval.0) => (),
            _ = api.wait_for_update_request() => (),
            Some(action) = actions.recv() => match action.as_ref() {
                "toggle_format" if output_alt.is_some() => {
                    alt_shown = !alt_shown;
                }
                _ => (),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_declares_thermometer_icon() {
        let plan = prepare(&Config::default()).unwrap();
        let ids: Vec<_> = plan.outputs.iter().map(|o| o.id).collect();
        assert_eq!(ids, ["main"]);
        let main = plan.output("main").unwrap();
        assert_eq!(main.single_icon("icon").unwrap(), "thermometer");
        assert!(main.format().contains_key("average"));
    }

    #[test]
    fn alt_output_exists_only_when_configured() {
        let plan = prepare(&Config::default()).unwrap();
        assert!(plan.output("alt").is_err());

        let config = Config {
            format_alt: Some(" $icon $min min ".parse().unwrap()),
            ..Config::default()
        };
        let plan = prepare(&config).unwrap();
        let alt = plan.output("alt").unwrap();
        assert!(alt.format().contains_key("min"));
        assert_eq!(alt.single_icon("icon").unwrap(), "thermometer");
    }

    #[test]
    fn both_outputs_guarantee_icon_and_temperatures() {
        let config = Config {
            format_alt: Some(" $icon $min min ".parse().unwrap()),
            ..Config::default()
        };
        let plan = prepare(&config).unwrap();
        for id in ["main", "alt"] {
            let output = plan.output(id).unwrap();
            assert_eq!(
                output.output().guaranteed_kind("icon"),
                Some(ValueKind::Icon),
                "{id}"
            );
            for placeholder in ["average", "min", "max"] {
                assert_eq!(
                    output.output().guaranteed_kind(placeholder),
                    Some(ValueKind::Number),
                    "{id} must guarantee {placeholder}"
                );
            }
        }
    }
}

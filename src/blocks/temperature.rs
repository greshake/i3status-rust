use std::time::Duration;
use std::process::Command;
use util::FormatTemplate;
use chan::Sender;
use scheduler::Task;

use block::{Block, ConfigBlock};
use config::Config;
use de::deserialize_duration;
use errors::*;
use widgets::button::ButtonWidget;
use widget::{I3BarWidget, State};
use input::{I3BarEvent, MouseButton};

use uuid::Uuid;

pub struct Temperature {
    text: ButtonWidget,
    output: String,
    collapsed: bool,
    id: String,
    update_interval: Duration,
    format: FormatTemplate,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct TemperatureConfig {
    /// Update interval in seconds
    #[serde(default = "TemperatureConfig::default_interval", deserialize_with = "deserialize_duration")]
    pub interval: Duration,

    /// Collapsed by default?
    #[serde(default = "TemperatureConfig::default_collapsed")]
    pub collapsed: bool,

    /// Format override
    #[serde(default = "TemperatureConfig::default_format")]
    pub format: String,
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
}

impl ConfigBlock for Temperature {
    type Config = TemperatureConfig;

    fn new(block_config: Self::Config, config: Config, _tx_update_request: Sender<Task>) -> Result<Self> {
        let id = format!("{}", Uuid::new_v4().to_simple());
        Ok(Temperature {
            update_interval: block_config.interval,
            text: ButtonWidget::new(config, &id).with_icon("thermometer"),
            output: String::new(),
            collapsed: block_config.collapsed,
            id,
            format: FormatTemplate::from_string(block_config.format)
                .block_error("temperature", "Invalid format specified for temperature")?,
        })
    }
}

impl Block for Temperature {
    fn update(&mut self) -> Result<Option<Duration>> {
        let output = Command::new("sensors")
            .args(&["-u"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
            .unwrap_or_else(|e| e.description().to_owned());

        let mut temperatures: Vec<i64> = Vec::new();

        for line in output.lines() {
            if line.starts_with("  temp") {
                let rest = &line[6..]
                    .split('_')
                    .flat_map(|x| x.split(' '))
                    .flat_map(|x| x.split('.'))
                    .collect::<Vec<_>>();

                if rest[1].starts_with("input") {
                    match rest[2].parse::<i64>() {
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

        if !temperatures.is_empty() {
            let max: i64 = *temperatures
                .iter()
                .max()
                .block_error("temperature", "failed to get max temperature")?;
            let min: i64 = *temperatures
                .iter()
                .min()
                .block_error("temperature", "failed to get min temperature")?;
            let avg: i64 = (temperatures.iter().sum::<i64>() as f64 / temperatures.len() as f64).round() as i64;

            let values = map!("{average}" => avg,
                              "{min}" => min,
                              "{max}" => max);

            self.output = self.format.render_static_str(&values)?;
            if !self.collapsed {
                self.text.set_text(self.output.clone());
            }

            self.text.set_state(match max {
                0...20 => State::Good,
                21...45 => State::Idle,
                46...60 => State::Info,
                61...80 => State::Warning,
                _ => State::Critical,
            });
        }

        Ok(Some(self.update_interval))
    }

    fn view(&self) -> Vec<&I3BarWidget> {
        vec![&self.text]
    }

    fn click(&mut self, e: &I3BarEvent) -> Result<()> {
        if let Some(ref name) = e.name {
            if name.as_str() == self.id && e.button == MouseButton::Left {
                self.collapsed = !self.collapsed;
                if self.collapsed {
                    self.text.set_text(String::new());
                } else {
                    self.text.set_text(self.output.clone());
                }
            }
        }

        Ok(())
    }

    fn id(&self) -> &str {
        &self.id
    }
}

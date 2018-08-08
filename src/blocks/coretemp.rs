use std::fs::File;
use std::time::Duration;
use std::path::PathBuf;
use std::io::prelude::*;

use util::FormatTemplate;
use chan::Sender;
use scheduler::Task;
use uuid::Uuid;
use glob::glob;

use block::{Block, ConfigBlock};
use config::Config;
use de::deserialize_duration;
use errors::*;
use widget::{I3BarWidget, State};
use widgets::text::TextWidget;

type Temperature = i32;

struct Thermometer {
    paths: Vec<PathBuf>,
}

impl Thermometer {
    pub fn new(pattern: &str) -> Result<Self> {
        let mut paths_vec = Vec::new();
        let paths = glob(pattern)
            .block_error("coretemp", "invalid file pattern")?;

        for path in paths {
            if path.is_ok() {
                paths_vec.push(path.unwrap())
            }
        }

        Ok(Thermometer {
            paths: paths_vec,
        })
    }

    pub fn measure(&self) -> Result<(Temperature, Temperature, Temperature)> {
        let mut temperatures = Vec::new();

        for path in self.paths.iter() {
            let mut file = File::open(path).unwrap();
            let mut temp_raw = String::new();

            file.read_to_string(&mut temp_raw)
                .block_error("coretemp", "failed to read file")?;

            let temp_raw : i32 = temp_raw.trim().parse()
                .block_error("coretemp", "failed to parse temperature")?;

            temperatures.push(temp_raw);
        }

        if temperatures.len() == 0 {
            return Err(BlockError(
                "coretemp".to_string(),
                "No temperatures found".to_string(),
            ))
        }

        let min = temperatures.iter().min().unwrap() / 1000;
        let max = temperatures.iter().max().unwrap() / 1000;
        let sum: i32 = temperatures.iter().sum();
        let avg = sum / temperatures.len() as i32 / 1000;

        Ok((min, max, avg))
    }
}

pub struct Coretemp {
    id: String,
    output: TextWidget,
    update_interval: Duration,
    format: FormatTemplate,
    thermometer: Thermometer,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct CoretempConfig {
    /// Update interval in seconds.
    #[serde(default = "CoretempConfig::default_interval", deserialize_with = "deserialize_duration")]
    pub interval: Duration,

    /// Format override.
    #[serde(default = "CoretempConfig::default_format")]
    pub format: String,

    /// Pattern override.
    #[serde(default = "CoretempConfig::default_pattern")]
    pub pattern: String,
}

impl CoretempConfig {
    fn default_interval() -> Duration {
        Duration::from_secs(5)
    }

    fn default_format() -> String {
        "{average}° avg, {max}° max".to_owned()
    }

    fn default_pattern() -> String {
        "/sys/devices/platform/coretemp.0/hwmon/*/temp*_input".to_owned()
    }
}

impl ConfigBlock for Coretemp {
    type Config = CoretempConfig;

    fn new(block_config: Self::Config, config: Config, _tx_update_request: Sender<Task>) -> Result<Self> {
        let id = Uuid::new_v4().simple().to_string();
        let thermometer = Thermometer::new(&block_config.pattern)?;

        Ok(Coretemp {
            id: id,
            output: TextWidget::new(config).with_icon("thermometer"),
            update_interval: block_config.interval,
            format: FormatTemplate::from_string(block_config.format)
                .block_error("coretemp", "Invalid format specified for temperature")?,
            thermometer: thermometer,
        })
    }
}

impl Block for Coretemp {
    fn update(&mut self) -> Result<Option<Duration>> {
        let (min, max, avg) = self.thermometer.measure()?;

        let values = map!("{average}" => avg,
                          "{min}" => min,
                          "{max}" => max);

        let text = self.format.render_static_str(&values)?;
        self.output.set_text(text);

        self.output.set_state(match max {
            0...20 => State::Good,
            21...45 => State::Idle,
            46...60 => State::Info,
            61...80 => State::Warning,
            _ => State::Critical,
        });

        Ok(Some(self.update_interval))
    }

    fn view(&self) -> Vec<&I3BarWidget> {
        vec![&self.output]
    }

    fn id(&self) -> &str {
        &self.id
    }
}

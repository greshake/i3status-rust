use chan::Sender;
use scheduler::Task;
use std::time::Duration;
use cpu_monitor::CpuInstant;

use block::{Block, ConfigBlock};
use config::Config;
use de::deserialize_duration;
use errors::*;
use widget::{I3BarWidget, State};
use widgets::text::TextWidget;

use std::fs::File;
use std::io::prelude::*;
use std::io::BufReader;
use std::fmt::Write;

use uuid::Uuid;

pub struct Cpu {
    utilization: TextWidget,
    // because we don't want to access /proc/stat until we update, this is an option
    prev_instant: Option<CpuInstant>,
    id: String,
    update_interval: Duration,
    minimum_info: f64,
    minimum_warning: f64,
    minimum_critical: f64,
    frequency: bool,
    show_utilization: bool,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct CpuConfig {
    /// Update interval in seconds
    #[serde(default = "CpuConfig::default_interval", deserialize_with = "deserialize_duration")]
    pub interval: Duration,

    /// Minimum usage, where state is set to info
    #[serde(default = "CpuConfig::default_info")]
    pub info: u64,

    /// Minimum usage, where state is set to warning
    #[serde(default = "CpuConfig::default_warning")]
    pub warning: u64,

    /// Minimum usage, where state is set to critical
    #[serde(default = "CpuConfig::default_critical")]
    pub critical: u64,

    /// Display frequency
    #[serde(default = "CpuConfig::default_frequency")]
    pub frequency: bool,

    /// Display utilization
    #[serde(default = "CpuConfig::default_utilization")]
    pub utilization: bool,
}

impl CpuConfig {
    fn default_interval() -> Duration {
        Duration::from_secs(1)
    }

    fn default_info() -> u64 {
        30
    }

    fn default_warning() -> u64 {
        60
    }

    fn default_critical() -> u64 {
        90
    }

    fn default_frequency() -> bool {
        false
    }

    fn default_utilization() -> bool {
        true
    }
}

impl ConfigBlock for Cpu {
    type Config = CpuConfig;

    fn new(block_config: Self::Config, config: Config, _tx_update_request: Sender<Task>) -> Result<Self> {
        Ok(Cpu {
            id: Uuid::new_v4().simple().to_string(),
            update_interval: block_config.interval,
            utilization: TextWidget::new(config).with_icon("cpu"),
            prev_instant: None,
            minimum_info: block_config.info as f64,
            minimum_warning: block_config.warning as f64,
            minimum_critical: block_config.critical as f64,
            frequency: block_config.frequency,
            show_utilization: block_config.utilization,
        })
    }
}

impl Block for Cpu {
    fn update(&mut self) -> Result<Option<Duration>> {
        let mut freq: f32 = 0.0;
        if self.frequency {
            let freq_file = File::open("/proc/cpuinfo")
                .block_error("cpu", "failed to read /proc/cpuinfo")?;
            let freq_file_content = BufReader::new(freq_file);
            let mut cores = 0;
            // read frequency of each cpu and calculate the average which we will display
            for line in freq_file_content.lines().scan((), |_, x| x.ok()) {
                if line.starts_with("cpu MHz") {
                    cores += 1;
                    let words = line.split(' ');
                    let last = words.last()
                        .expect("failed to get last word of line while getting cpu frequency");
                    let numb = last.parse::<f32>()
                        .expect("failed to parse String to f32 while getting cpu frequency");
                    freq += numb;
                }
            }
            // get the average
            freq = (freq / (cores as f32) / 1000.0) as f32;
        }

        let next_instant = CpuInstant::now().block_error("cpu", "Error getting cpu usage info")?;

        let utilization = if let Some(prev_instant) = self.prev_instant {
            let utilization = (next_instant - prev_instant).non_idle() * 100.;
            self.utilization.set_state(match utilization {
                x if x > self.minimum_critical => State::Critical,
                x if x > self.minimum_warning => State::Warning,
                x if x > self.minimum_info => State::Info,
                _ => State::Idle,
            });
            Some(utilization)
        } else {
            self.utilization.set_state(State::Idle);
            None
        };
        let mut new_text = String::new();
        if self.show_utilization {
            if let Some(u) = utilization {
                write!(new_text, "{:3.0}%", u).unwrap(); // unfailable
            } else {
                write!(new_text, "   %").unwrap();
            }
        }
        if self.frequency && self.show_utilization {
            write!(new_text, " ").unwrap();
        }
        if self.frequency {
            write!(new_text, "{:.1}GHz", freq).unwrap();
        }
        self.utilization.set_text(new_text);
        self.prev_instant = Some(next_instant);
        Ok(Some(self.update_interval))
    }

    fn view(&self) -> Vec<&I3BarWidget> {
        vec![&self.utilization]
    }

    fn id(&self) -> &str {
        &self.id
    }
}

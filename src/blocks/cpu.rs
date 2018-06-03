use chan::Sender;
use scheduler::Task;
use std::time::Duration;

use block::{Block, ConfigBlock};
use config::Config;
use de::deserialize_duration;
use errors::*;
use widget::{I3BarWidget, State};
use widgets::text::TextWidget;

use std::fs::File;
use std::io::prelude::*;
use std::io::BufReader;

use uuid::Uuid;

pub struct Cpu {
    utilization: TextWidget,
    prev_idle: u64,
    prev_non_idle: u64,
    id: String,
    update_interval: Duration,
    minimum_info: u64,
    minimum_warning: u64,
    minimum_critical: u64,
    frequency: bool,
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
}

impl ConfigBlock for Cpu {
    type Config = CpuConfig;

    fn new(block_config: Self::Config, config: Config, _tx_update_request: Sender<Task>) -> Result<Self> {
        Ok(Cpu {
            id: format!("{}", Uuid::new_v4().to_simple()),
            update_interval: block_config.interval,
            utilization: TextWidget::new(config).with_icon("cpu"),
            prev_idle: 0,
            prev_non_idle: 0,
            minimum_info: block_config.info,
            minimum_warning: block_config.warning,
            minimum_critical: block_config.critical,
            frequency: block_config.frequency,
        })
    }
}

impl Block for Cpu {
    fn update(&mut self) -> Result<Option<Duration>> {
        let f = File::open("/proc/stat").block_error("cpu", "Your system doesn't support /proc/stat")?;
        let f = BufReader::new(f);

        let mut freq: f32 = 0.0;
        if self.frequency {
            let freq_file = File::open("/proc/cpuinfo").block_error("cpu", "failed to read /proc/cpuinfo")?;
            let freq_file_content = BufReader::new(freq_file);
            let mut cores = 0;
            // read frequency of each cpu and calculate the average which we will display
            for line in freq_file_content.lines().scan((), |_, x| x.ok()) {
                if line.starts_with("cpu MHz") {
                    cores += 1;
                    let words = line.split(' ');
                    let last = words.last().expect("failed to get last word of line while getting cpu frequency");
                    let numb = last.parse::<f32>().expect("failed to parse String to f32 while getting cpu frequency");
                    freq += numb;
                }
            }
            // get the average
            freq = (freq / (cores as f32) / 1000.0) as f32;
        }
        let mut utilization = 0;

        for line in f.lines().scan((), |_, x| x.ok()) {
            if line.starts_with("cpu ") {
                let data: Vec<u64> = (&line).split(' ').collect::<Vec<&str>>().iter().skip(2).filter_map(|x| x.parse::<u64>().ok()).collect::<Vec<_>>();

                // idle = idle + iowait
                let idle = data[3] + data[4];
                let non_idle = data[0] + // user
                                data[1] + // nice
                                data[2] + // system
                                data[5] + // irq
                                data[6] + // softirq
                                data[7]; // steal

                let prev_total = self.prev_idle + self.prev_non_idle;
                let total = idle + non_idle;

                // This check is needed because the new values may be reset, for
                // example after hibernation.

                let (total_delta, idle_delta) = if prev_total < total && self.prev_idle <= idle {
                    (total - prev_total, idle - self.prev_idle)
                } else {
                    (1, 1)
                };

                utilization = (((total_delta - idle_delta) as f64 / total_delta as f64) * 100.) as u64;

                self.prev_idle = idle;
                self.prev_non_idle = non_idle;
            }
        }

        self.utilization.set_state(match utilization {
            x if x > self.minimum_critical => State::Critical,
            x if x > self.minimum_warning => State::Warning,
            x if x > self.minimum_info => State::Info,
            _ => State::Idle,
        });
        if self.frequency {
            let frequency = format!("{:.*}", 1, freq);
            self.utilization.set_text(format!("{:02}% {}GHz", utilization, frequency));
        } else {
            self.utilization.set_text(format!("{:02}%", utilization));
        }
        Ok(Some(self.update_interval))
    }

    fn view(&self) -> Vec<&I3BarWidget> {
        vec![&self.utilization]
    }

    fn id(&self) -> &str {
        &self.id
    }
}

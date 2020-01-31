use crate::scheduler::Task;
use crate::util::FormatTemplate;
use crossbeam_channel::Sender;
use serde_derive::Deserialize;
use std::time::Duration;

use crate::blocks::{Block, ConfigBlock};
use crate::config::Config;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::widget::{I3BarWidget, State};
use crate::widgets::text::TextWidget;

use std::fs::File;
use std::io::prelude::*;
use std::io::BufReader;

use uuid::Uuid;

pub struct Cpu {
    output: TextWidget,
    prev_idles: [u64; 32],
    prev_non_idles: [u64; 32],
    id: String,
    update_interval: Duration,
    minimum_info: u64,
    minimum_warning: u64,
    minimum_critical: u64,
    format: FormatTemplate,
    has_barchart: bool,
    has_frequency: bool,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct CpuConfig {
    /// Update interval in seconds
    #[serde(
        default = "CpuConfig::default_interval",
        deserialize_with = "deserialize_duration"
    )]
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

    /// Format override
    #[serde(default = "CpuConfig::default_format")]
    pub format: String,
}

impl CpuConfig {
    fn default_format() -> String {
        "{utilization}%".to_owned()
    }

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

    fn new(
        block_config: Self::Config,
        config: Config,
        _tx_update_request: Sender<Task>,
    ) -> Result<Self> {
        let mut format = block_config.format;
        if block_config.frequency {
            format = "{utilization}% {frequency}GHz".into();
        }

        Ok(Cpu {
            id: Uuid::new_v4().to_simple().to_string(),
            update_interval: block_config.interval,
            output: TextWidget::new(config).with_icon("cpu"),
            prev_idles: [0; 32],
            prev_non_idles: [0; 32],
            minimum_info: block_config.info,
            minimum_warning: block_config.warning,
            minimum_critical: block_config.critical,
            format: FormatTemplate::from_string(&format)
                .block_error("cpu", "Invalid format specified for cpu")?,
            has_frequency: format.contains("{frequency}"),
            has_barchart: format.contains("{barchart}"),
        })
    }
}

impl Block for Cpu {
    fn update(&mut self) -> Result<Option<Duration>> {
        let f = File::open("/proc/stat")
            .block_error("cpu", "Your system doesn't support /proc/stat")?;
        let f = BufReader::new(f);

        let mut n_cpu = 0;
        let mut freq: f32 = 0.0;
        if self.has_frequency {
            let freq_file =
                File::open("/proc/cpuinfo").block_error("cpu", "failed to read /proc/cpuinfo")?;
            let freq_file_content = BufReader::new(freq_file);
            // read frequency of each cpu and calculate the average which we will display
            for line in freq_file_content.lines().scan((), |_, x| x.ok()) {
                if line.starts_with("cpu MHz") {
                    let words = line.split(' ');
                    let last = words
                        .last()
                        .expect("failed to get last word of line while getting cpu frequency");
                    let numb = last
                        .parse::<f32>()
                        .expect("failed to parse String to f32 while getting cpu frequency");
                    freq += numb;
                    n_cpu += 1;
                }
            }
            // get the average
            freq = (freq / (n_cpu as f32) / 1000.0) as f32;
        }

        // Read data from a maximum of 32 CPU cores, if a barchart is displayed
        let max_cpus = if self.has_barchart { 32 } else { 1 };
        let mut cpu_utilizations = vec![0.0; max_cpus];

        let mut cpu_i = 0;
        for line in f.lines().scan((), |_, x| x.ok()) {
            if line.starts_with("cpu") {
                let data: Vec<u64> = (&line)
                    .split(' ')
                    .collect::<Vec<&str>>()
                    .iter()
                    .skip(if cpu_i == 0 { 2 } else { 1 })
                    .filter_map(|x| x.parse::<u64>().ok())
                    .collect::<Vec<_>>();

                // idle = idle + iowait
                let idle = data[3] + data[4];
                let non_idle = data[0] + // user
                                data[1] + // nice
                                data[2] + // system
                                data[5] + // irq
                                data[6] + // softirq
                                data[7]; // steal

                let prev_total = self.prev_idles[cpu_i] + self.prev_non_idles[cpu_i];
                let total = idle + non_idle;

                // This check is needed because the new values may be reset, for
                // example after hibernation.

                let (total_delta, idle_delta) =
                    if prev_total < total && self.prev_idles[cpu_i] <= idle {
                        (total - prev_total, idle - self.prev_idles[cpu_i])
                    } else {
                        (1, 1)
                    };

                cpu_utilizations[cpu_i] = (total_delta - idle_delta) as f64 / total_delta as f64;

                self.prev_idles[cpu_i] = idle;
                self.prev_non_idles[cpu_i] = non_idle;
                cpu_i += 1;
                if cpu_i >= max_cpus {
                    break;
                };
            }
        }

        let avg_utilization = (100.0 * cpu_utilizations[0]) as u64;

        self.output.set_state(match avg_utilization {
            x if x > self.minimum_critical => State::Critical,
            x if x > self.minimum_warning => State::Warning,
            x if x > self.minimum_info => State::Info,
            _ => State::Idle,
        });

        let mut barchart = String::new();

        if self.has_barchart {
            const BOXCHARS: &[char] = &['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

            for i in 1..cpu_i {
                barchart.push(
                    BOXCHARS[((7.5 * cpu_utilizations[i]) as usize)
                        // TODO: Replace with .clamp once the feature is stable
                        // upper bound just in case the value is negative, e.g. USIZE MAX after conversion
                        .min(BOXCHARS.len() - 1)],
                );
            }
        }

        let values = map!("{frequency}" => format!("{:.*}", 1, freq),
                          "{barchart}" => barchart,
                          "{utilization}" => format!("{:02}", avg_utilization));

        self.output
            .set_text(self.format.render_static_str(&values)?);

        Ok(Some(self.update_interval))
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.output]
    }

    fn id(&self) -> &str {
        &self.id
    }
}

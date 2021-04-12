use std::time::Duration;

use async_trait::async_trait;
use crossbeam_channel::Sender;
use futures::{future, StreamExt};
use serde_derive::Deserialize;
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio_stream::wrappers::LinesStream;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::SharedConfig;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::formatting::value::Value;
use crate::formatting::FormatTemplate;
use crate::scheduler::Task;
use crate::widgets::text::TextWidget;
use crate::widgets::{I3BarWidget, State};

pub struct Cpu {
    id: usize,
    prev_util: Vec<(u64, u64)>,
    update_interval: Duration,
    minimum_info: u64,
    minimum_warning: u64,
    minimum_critical: u64,
    format: FormatTemplate,
    shared_config: SharedConfig,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct CpuConfig {
    /// Update interval in seconds
    #[serde(deserialize_with = "deserialize_duration")]
    pub interval: Duration,

    /// Minimum usage, where state is set to info
    pub info: u64,

    /// Minimum usage, where state is set to warning
    pub warning: u64,

    /// Minimum usage, where state is set to critical
    pub critical: u64,

    /// Format override
    pub format: String,
}

impl Default for CpuConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(1),
            info: 30,
            warning: 60,
            critical: 90,
            format: "{utilization}".to_string(),
        }
    }
}

impl ConfigBlock for Cpu {
    type Config = CpuConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        shared_config: SharedConfig,
        _tx_update_request: Sender<Task>,
    ) -> Result<Self> {
        Ok(Cpu {
            id,
            update_interval: block_config.interval,
            prev_util: Vec::with_capacity(32),
            minimum_info: block_config.info,
            minimum_warning: block_config.warning,
            minimum_critical: block_config.critical,
            format: FormatTemplate::from_string(&block_config.format)
                .block_error("cpu", "Invalid format specified for cpu")?,
            shared_config,
        })
    }
}

#[async_trait(?Send)]
impl Block for Cpu {
    fn update_interval(&self) -> Update {
        self.update_interval.into()
    }

    async fn render(&mut self) -> Result<Vec<Box<dyn I3BarWidget>>> {
        let mut widget =
            TextWidget::new(self.id, 0, self.shared_config.clone()).with_icon("cpu")?;

        // Read frequencies (read in MHz, store in Hz)
        let file = File::open("/proc/cpuinfo")
            .await
            .block_error("cpu", "failed to read /proc/cpuinfo")?;

        let freqs: Vec<_> = LinesStream::new(BufReader::new(file).lines())
            .filter_map(|res| async move { res.ok() })
            .filter(|line| future::ready(line.starts_with("cpu MHz")))
            .map(|line| {
                line.split(' ')
                    .last()
                    .expect("failed to get last word of line while getting cpu frequency")
                    .parse::<f64>()
                    .expect("failed to parse String to f64 while getting cpu frequency")
                    * 1e6 // convert to Hz
            })
            .collect()
            .await;

        let freqs_avg = freqs.iter().sum::<f64>() / freqs.len() as f64;

        // Read utilizations
        let file = File::open("/proc/stat")
            .await
            .block_error("cpu", "Your system doesn't support /proc/stat")?;

        let utilizations: Vec<_> = LinesStream::new(BufReader::new(file).lines())
            .filter_map(|res| async move { res.ok() })
            .filter(|line| future::ready(line.starts_with("cpu")))
            .enumerate()
            .map(|(i, line)| {
                let cols: Vec<u64> = line
                    .split_whitespace()
                    .filter_map(|x| x.parse::<u64>().ok())
                    .collect();

                match *cols.as_slice() {
                    [user, nice, system, idle, iowait, irq, softirq, steal, ..] => {
                        let idle = idle + iowait;
                        let non_idle = user + nice + system + irq + softirq + steal;

                        let (prev_idles, prev_non_idles) = {
                            if self.prev_util.len() <= i {
                                self.prev_util.push((0, 0));
                                (0, 0)
                            } else {
                                self.prev_util[i]
                            }
                        };

                        let prev_total = prev_idles + prev_non_idles;
                        let total = idle + non_idle;

                        // This check is needed because the new values may be reset, for
                        // example after hibernation.
                        let (total_delta, idle_delta) = {
                            if prev_total < total && prev_idles <= idle {
                                (total - prev_total, idle - prev_idles)
                            } else {
                                (1, 1)
                            }
                        };

                        self.prev_util[i] = (idle, non_idle);
                        ((total_delta - idle_delta) as f64 / total_delta as f64).clamp(0., 1.)
                    }
                    _ => panic!("not enough columns in CPU block"),
                }
            })
            .collect()
            .await;

        let (avg, utilizations) = utilizations.split_first().unwrap();
        let avg_utilization = avg * 100.;

        widget.set_state(match avg_utilization as u64 {
            x if x > self.minimum_critical => State::Critical,
            x if x > self.minimum_warning => State::Warning,
            x if x > self.minimum_info => State::Info,
            _ => State::Idle,
        });

        let mut barchart = String::new();
        const BOXCHARS: &[char] = &['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
        for utilization in utilizations {
            barchart.push(BOXCHARS[(7.5 * utilization) as usize]);
        }

        let mut values = map!(
            "frequency" => Value::from_float(freqs_avg).hertz(),
            "barchart" => Value::from_string(barchart),
            "utilization" => Value::from_integer(avg_utilization as i64).percents(),
        );
        let mut frequency_keys = vec![]; // There should be a better way to dynamically crate keys?
        for i in 0..freqs.len() {
            frequency_keys.push(format!("frequency{}", i + 1));
        }
        for (i, freq) in freqs.iter().enumerate() {
            values.insert(&frequency_keys[i], Value::from_float(*freq).hertz());
        }
        let mut utilization_keys = vec![]; // There should be a better way to dynamically crate keys?
        for i in 0..utilizations.len() {
            utilization_keys.push(format!("utilization{}", i + 1));
        }
        for (i, utilization) in utilizations.iter().enumerate() {
            values.insert(
                &utilization_keys[i],
                Value::from_integer((utilization * 100.) as i64).percents(),
            );
        }

        widget.set_text(self.format.render(&values)?);
        Ok(vec![Box::new(widget)])
    }

    fn id(&self) -> usize {
        self.id
    }
}

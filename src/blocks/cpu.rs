use std::fs::{read_to_string, File};
use std::io::prelude::*;
use std::io::BufReader;
use std::time::Duration;

use crossbeam_channel::Sender;
use serde_derive::Deserialize;

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
    output: TextWidget,
    prev_util: Vec<(u64, u64)>,
    update_interval: Duration,
    minimum_info: u64,
    minimum_warning: u64,
    minimum_critical: u64,
    format: FormatTemplate,
    boost_icon_on: String,
    boost_icon_off: String,
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
    pub format: FormatTemplate,
}

impl Default for CpuConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(1),
            info: 30,
            warning: 60,
            critical: 90,
            format: FormatTemplate::default(),
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
            boost_icon_on: shared_config.get_icon("cpu_boost_on")?,
            boost_icon_off: shared_config.get_icon("cpu_boost_off")?,
            output: TextWidget::new(id, 0, shared_config).with_icon("cpu")?,
            format: block_config.format.with_default("{utilization}")?,
        })
    }
}

impl Block for Cpu {
    fn update(&mut self) -> Result<Option<Update>> {
        // Read frequencies (read in MHz, store in Hz)
        let mut freqs = Vec::with_capacity(32);
        let mut freqs_avg = 0.0;
        let freqs_f =
            File::open("/proc/cpuinfo").block_error("cpu", "failed to read /proc/cpuinfo")?;
        for line in BufReader::new(freqs_f).lines().scan((), |_, x| x.ok()) {
            if line.starts_with("cpu MHz") {
                let words = line.split(' ');
                let last = words
                    .last()
                    .expect("failed to get last word of line while getting cpu frequency");
                let numb = last
                    .parse::<f64>()
                    .expect("failed to parse String to f64 while getting cpu frequency")
                    * 1e6; // convert to Hz
                freqs.push(numb);
                freqs_avg += numb;
            }
        }
        freqs_avg /= freqs.len() as f64;

        // Read utilizations
        let mut utilizations = Vec::with_capacity(32);
        let utilizations_f = File::open("/proc/stat")
            .block_error("cpu", "Your system doesn't support /proc/stat")?;
        for (i, line) in BufReader::new(utilizations_f)
            .lines()
            .scan((), |_, x| x.ok())
            .enumerate()
        {
            if line.starts_with("cpu") {
                let data: Vec<u64> = line
                    .split_whitespace()
                    .filter_map(|x| x.parse::<u64>().ok())
                    .collect();

                // idle = idle + iowait
                let idle = data[3] + data[4];
                let non_idle = data[0] + // user
                                data[1] + // nice
                                data[2] + // system
                                data[5] + // irq
                                data[6] + // softirq
                                data[7]; // steal

                let (prev_idles, prev_non_idles) = if self.prev_util.len() <= i {
                    self.prev_util.push((0, 0));
                    (0, 0)
                } else {
                    self.prev_util[i]
                };

                let prev_total = prev_idles + prev_non_idles;
                let total = idle + non_idle;

                // This check is needed because the new values may be reset, for
                // example after hibernation.
                let (total_delta, idle_delta) = if prev_total < total && prev_idles <= idle {
                    (total - prev_total, idle - prev_idles)
                } else {
                    (1, 1)
                };

                utilizations
                    .push(((total_delta - idle_delta) as f64 / total_delta as f64).clamp(0., 1.));

                self.prev_util[i] = (idle, non_idle);
            }
        }

        let (avg, utilizations) = utilizations.split_first().unwrap();
        let avg_utilization = avg * 100.;

        self.output.set_state(match avg_utilization as u64 {
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

        let boost = match boost_status() {
            Some(true) => self.boost_icon_on.clone(),
            Some(false) => self.boost_icon_off.clone(),
            _ => String::new(),
        };

        let mut values = map_to_owned!(
            "frequency" => Value::from_float(freqs_avg).hertz(),
            "barchart" => Value::from_string(barchart),
            "utilization" => Value::from_integer(avg_utilization as i64).percents(),
            "boost" => Value::from_string(boost),
        );
        for (i, freq) in freqs.into_iter().enumerate() {
            values.insert(
                format!("frequency{}", i + 1),
                Value::from_float(freq).hertz(),
            );
        }
        for (i, utilization) in utilizations.iter().enumerate() {
            values.insert(
                format!("utilization{}", i + 1),
                Value::from_integer((utilization * 100.) as i64).percents(),
            );
        }

        self.output.set_texts(self.format.render(&values)?);

        Ok(Some(self.update_interval.into()))
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.output]
    }

    fn id(&self) -> usize {
        self.id
    }
}

/// Read the cpu turbo boost status from kernel sys interface
/// or intel pstate interface
fn boost_status() -> Option<bool> {
    if let Ok(boost) = read_to_string("/sys/devices/system/cpu/cpufreq/boost") {
        return Some(boost.starts_with('1'));
    } else if let Ok(no_turbo) = read_to_string("/sys/devices/system/cpu/intel_pstate/no_turbo") {
        return Some(no_turbo.starts_with('0'));
    }
    None
}

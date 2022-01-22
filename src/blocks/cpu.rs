//! CPU statistics
//!
//! # Configuration
//!
//! Key | Values | Required | Default
//! ----|--------|----------|--------
//! `format` | A string to customise the output of this block. See below for available placeholders. | No | `"$utilization"`
//! `format_alt` | If set, block will switch between `format` and `format_alt` on every click | No | None
//! `interval` | Update interval in seconds | No | `5`
//!
//! Placeholder      | Value                                                          | Type   | Unit
//! -----------------|----------------------------------------------------------------|--------|---------------
//! `utilization`    | Average CPU utilization                                        | Number | %
//! `utilization<N>` | Utilization of Nth logical CPU                                 | Number | %
//! `barchart`       | Utilization of all logical CPUs presented as a barchart        | Text   | -
//! `frequency`      | Average CPU frequency                                          | Number | Hz
//! `frequency<N>`   | Frequency of Nth logical CPU                                   | Number | Hz
//! `boost`          | CPU turbo boost status (may be absent if CPU is not supported) | Text   | -
//!
//! # Example
//!
//! ```toml
//! [[block]]
//! block = "cpu"
//! interval = 1
//! format = "$barchart.str() $utilization.eng()"
//! format_alt = "$frequency.eng() \\|$boost.str()"
//! ```
//!
//! # Icons Used
//! - `cpu`
//! - `cpu_boost_on`
//! - `cpu_boost_off`

use std::path::Path;
use std::str::FromStr;

use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, BufReader};

use super::prelude::*;
use crate::util::read_file;

const CPU_BOOST_PATH: &str = "/sys/devices/system/cpu/cpufreq/boost";
const CPU_NO_TURBO_PATH: &str = "/sys/devices/system/cpu/intel_pstate/no_turbo";

#[derive(Deserialize, Debug, Derivative)]
#[serde(deny_unknown_fields, default)]
#[derivative(Default)]
struct CpuConfig {
    format: FormatConfig,
    format_alt: Option<FormatConfig>,
    #[derivative(Default(value = "5.into()"))]
    interval: Seconds,
}

pub async fn run(config: toml::Value, mut api: CommonApi) -> Result<()> {
    let mut events = api.get_events().await?;
    let config = CpuConfig::deserialize(config).config_error()?;
    let mut format = config.format.with_default("$utilization")?;
    let mut format_alt = match config.format_alt {
        Some(f) => Some(f.with_default("")?),
        None => None,
    };
    api.set_format(format.clone());

    api.set_icon("cpu")?;
    let boost_icon_on = api.get_icon("cpu_boost_on")?;
    let boost_icon_off = api.get_icon("cpu_boost_off")?;

    // Store previous /proc/stat state
    let mut cputime = read_proc_stat().await?;
    let cores = cputime.1.len();

    loop {
        let freqs = read_frequencies().await?;
        let freq_avg = freqs.iter().sum::<f64>() / (freqs.len() as f64);

        // Compute utilizations
        let new_cputime = read_proc_stat().await?;
        let utilization_avg = new_cputime.0.utilization(cputime.0);
        let mut utilizations = Vec::new();
        if new_cputime.1.len() != cores {
            return Err(Error::new("new cputime length is incorrect"));
        }
        for i in 0..cores {
            utilizations.push(new_cputime.1[i].utilization(cputime.1[i]));
        }
        cputime = new_cputime;

        // Create barchart indicating per-core utilization
        let mut barchart = String::new();
        const BOXCHARS: &[char] = &['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
        for utilization in &utilizations {
            barchart.push(BOXCHARS[(7.5 * utilization) as usize]);
        }

        // Read boot state on intel CPUs
        let boost = boost_status().await.map(|status| match status {
            true => boost_icon_on.clone(),
            false => boost_icon_off.clone(),
        });

        let mut values = map!(
            "barchart" => Value::text(barchart),
            "frequency" => Value::hertz(freq_avg),
            "utilization" => Value::percents(utilization_avg * 100.),
        );
        boost.map(|b| values.insert("boost".into(), Value::Icon(b)));
        for (i, freq) in freqs.iter().enumerate() {
            values.insert(format!("frequency{}", i + 1).into(), Value::hertz(*freq));
        }
        for (i, utilization) in utilizations.iter().enumerate() {
            values.insert(
                format!("utilization{}", i + 1).into(),
                Value::percents(utilization * 100.),
            );
        }

        api.set_values(values);
        api.set_state(match utilization_avg {
            x if x > 0.9 => State::Critical,
            x if x > 0.6 => State::Warning,
            x if x > 0.3 => State::Info,
            _ => State::Idle,
        });
        api.flush().await?;

        tokio::select! {
            _ = sleep(config.interval.0) => (),
            Some(BlockEvent::Click(click)) = events.recv() => {
                if click.button == MouseButton::Left {
                    if let Some(ref mut format_alt) = format_alt {
                        std::mem::swap(format_alt, &mut format);
                        api.set_format(format.clone());
                    }
                }
            }
        }
    }
}

// Read frequencies (read in MHz, store in Hz)
async fn read_frequencies() -> Result<Vec<f64>> {
    let mut freqs = Vec::with_capacity(32);

    let file = File::open("/proc/cpuinfo")
        .await
        .error("failed to read /proc/cpuinfo")?;
    let mut file = BufReader::new(file);

    let mut line = StdString::new();
    while file
        .read_line(&mut line)
        .await
        .error("failed to read /proc/cpuinfo")?
        != 0
    {
        if line.starts_with("cpu MHz") {
            let slice = line
                .trim_end()
                .trim_start_matches(|c: char| !c.is_digit(10));
            freqs.push(f64::from_str(slice).error("failed to parse /proc/cpuinfo")? * 1e6);
        }
        line.clear();
    }

    Ok(freqs)
}

#[derive(Debug, Clone, Copy)]
struct CpuTime {
    idle: u64,
    non_idle: u64,
}

impl CpuTime {
    fn from_str(s: &str) -> Option<Self> {
        let mut s = s.trim().split_ascii_whitespace();
        let user = u64::from_str(s.next()?).ok()?;
        let nice = u64::from_str(s.next()?).ok()?;
        let system = u64::from_str(s.next()?).ok()?;
        let idle = u64::from_str(s.next()?).ok()?;
        let iowait = u64::from_str(s.next()?).ok()?;
        let irq = u64::from_str(s.next()?).ok()?;
        let softirq = u64::from_str(s.next()?).ok()?;

        Some(Self {
            idle: idle + iowait,
            non_idle: user + nice + system + irq + softirq,
        })
    }

    fn utilization(&self, old: Self) -> f64 {
        let elapsed = (self.idle + self.non_idle) as f64 - (old.idle + old.non_idle) as f64;
        ((self.non_idle - old.non_idle) as f64 / elapsed).clamp(0., 1.)
    }
}

async fn read_proc_stat() -> Result<(CpuTime, Vec<CpuTime>)> {
    let mut utilizations = Vec::with_capacity(32);
    let mut total = None;

    let file = File::open("/proc/stat")
        .await
        .error("failed to read /proc/stat")?;
    let mut file = BufReader::new(file);

    let mut line = StdString::new();
    while file
        .read_line(&mut line)
        .await
        .error("failed to read /proc/sta")?
        != 0
    {
        // Total time
        let data = line.trim_start_matches(|c: char| !c.is_ascii_whitespace());
        if line.starts_with("cpu ") {
            total = Some(CpuTime::from_str(data).error("failed to parse /proc/stat")?);
        } else if line.starts_with("cpu") {
            utilizations.push(CpuTime::from_str(data).error("failed to parse /proc/stat")?);
        }
        line.clear();
    }

    Ok((total.error("failed to parse /proc/stat")?, utilizations))
}

/// Read the cpu turbo boost status from kernel sys interface
/// or intel pstate interface
async fn boost_status() -> Option<bool> {
    if let Ok(boost) = read_file(Path::new(CPU_BOOST_PATH)).await {
        Some(boost.starts_with('1'))
    } else if let Ok(no_turbo) = read_file(Path::new(CPU_NO_TURBO_PATH)).await {
        Some(no_turbo.starts_with('0'))
    } else {
        None
    }
}

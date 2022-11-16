//! CPU statistics
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `format` | A string to customise the output of this block. See below for available placeholders. | `" $icon $utilization "`
//! `format_alt` | If set, block will switch between `format` and `format_alt` on every click | `None`
//! `interval` | Update interval in seconds | `5`
//!
//! Placeholder      | Value                                                          | Type   | Unit
//! -----------------|----------------------------------------------------------------|--------|---------------
//! `icon`           | A static icon                                                  | Icon   | -
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
//! format = " $icon $barchart $utilization "
//! format_alt = " $icon $frequency{ $boost|} "
//! ```
//!
//! # Icons Used
//! - `cpu`
//! - `cpu_boost_on`
//! - `cpu_boost_off`

use std::str::FromStr;

use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, BufReader};

use super::prelude::*;
use crate::util::read_file;

const CPU_BOOST_PATH: &str = "/sys/devices/system/cpu/cpufreq/boost";
const CPU_NO_TURBO_PATH: &str = "/sys/devices/system/cpu/intel_pstate/no_turbo";

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(default)]
pub struct Config {
    format: FormatConfig,
    format_alt: Option<FormatConfig>,
    #[default(5.into())]
    interval: Seconds,
}

pub async fn run(config: Config, mut api: CommonApi) -> Result<()> {
    let mut format = config.format.with_default(" $icon $utilization ")?;
    let mut format_alt = match config.format_alt {
        Some(f) => Some(f.with_default("")?),
        None => None,
    };

    let mut widget = Widget::new().with_format(format.clone());

    let boost_icon_on = api.get_icon("cpu_boost_on")?;
    let boost_icon_off = api.get_icon("cpu_boost_off")?;

    // Store previous /proc/stat state
    let mut cputime = read_proc_stat().await?;
    let cores = cputime.1.len();

    let mut timer = config.interval.timer();

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
            "icon" => Value::icon(api.get_icon("cpu")?),
            "barchart" => Value::text(barchart),
            "frequency" => Value::hertz(freq_avg),
            "utilization" => Value::percents(utilization_avg * 100.),
        );
        boost.map(|b| values.insert("boost".into(), Value::icon(b)));
        for (i, freq) in freqs.iter().enumerate() {
            values.insert(format!("frequency{}", i + 1).into(), Value::hertz(*freq));
        }
        for (i, utilization) in utilizations.iter().enumerate() {
            values.insert(
                format!("utilization{}", i + 1).into(),
                Value::percents(utilization * 100.),
            );
        }

        widget.set_values(values);
        widget.state = match utilization_avg {
            x if x > 0.9 => State::Critical,
            x if x > 0.6 => State::Warning,
            x if x > 0.3 => State::Info,
            _ => State::Idle,
        };
        api.set_widget(&widget).await?;

        loop {
            select! {
                _ = timer.tick() => break,
                Click(click) = api.event() => {
                    if click.button == MouseButton::Left {
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

// Read frequencies (read in MHz, store in Hz)
async fn read_frequencies() -> Result<Vec<f64>> {
    let mut freqs = Vec::with_capacity(32);

    let file = File::open("/proc/cpuinfo")
        .await
        .error("failed to read /proc/cpuinfo")?;
    let mut file = BufReader::new(file);

    let mut line = String::new();
    while file
        .read_line(&mut line)
        .await
        .error("failed to read /proc/cpuinfo")?
        != 0
    {
        if line.starts_with("cpu MHz") {
            let slice = line
                .trim_end()
                .trim_start_matches(|c: char| !c.is_ascii_digit());
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

    let mut line = String::new();
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
    if let Ok(boost) = read_file(CPU_BOOST_PATH).await {
        Some(boost.starts_with('1'))
    } else if let Ok(no_turbo) = read_file(CPU_NO_TURBO_PATH).await {
        Some(no_turbo.starts_with('0'))
    } else {
        None
    }
}

//! CPU statistics
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `format` | A string to customise the output of this block. See below for available placeholders. | `" $icon $utilization "`
//! `format_alt` | If set, block will switch between `format` and `format_alt` on every click | `None`
//! `interval` | Update interval in seconds | `5`
//! `info_cpu` | Percentage of CPU usage, where state is set to info | `30.0`
//! `warning_cpu` | Percentage of CPU usage, where state is set to warning | `60.0`
//! `critical_cpu` | Percentage of CPU usage, where state is set to critical | `90.0`
//!
//! Placeholder      | Value                                                                | Type   | Unit
//! -----------------|----------------------------------------------------------------------|--------|---------------
//! `icon`           | An icon                                                              | Icon   | -
//! `utilization`    | Average CPU utilization                                              | Number | %
//! `utilization<N>` | Utilization of Nth logical CPU                                       | Number | %
//! `barchart`       | Utilization of all logical CPUs presented as a barchart              | Text   | -
//! `frequency`      | Average CPU frequency (may be absent if CPU is not supported)        | Number | Hz
//! `frequency<N>`   | Frequency of Nth logical CPU (may be absent if CPU is not supported) | Number | Hz
//! `max_frequency`  | Max frequency of all logical CPUs                                    | Number | Hz
//! `boost`          | CPU turbo boost status (may be absent if CPU is not supported)       | Text   | -
//!
//! Action          | Description                               | Default button
//! ----------------|-------------------------------------------|---------------
//! `toggle_format` | Toggles between `format` and `format_alt` | Left
//!
//! # Example
//!
//! ```toml
//! [[block]]
//! block = "cpu"
//! interval = 1
//! format = " $icon $barchart $utilization "
//! format_alt = " $icon $frequency{ $boost|} "
//! info_cpu = 20
//! warning_cpu = 50
//! critical_cpu = 90
//! ```
//!
//! # Icons Used
//! - `cpu` (as a progression)
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
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub format: FormatConfig,
    pub format_alt: Option<FormatConfig>,
    #[default(5.into())]
    pub interval: Seconds,
    #[default(30.0)]
    pub info_cpu: f64,
    #[default(60.0)]
    pub warning_cpu: f64,
    #[default(90.0)]
    pub critical_cpu: f64,
}

pub async fn run(config: &Config, api: &CommonApi) -> Result<()> {
    let mut actions = api.get_actions()?;
    api.set_default_actions(&[(MouseButton::Left, None, "toggle_format")])?;

    let mut format = config.format.with_default(" $icon $utilization ")?;
    let mut format_alt = match &config.format_alt {
        Some(f) => Some(f.with_default("")?),
        None => None,
    };

    // Store previous /proc/stat state
    let mut cputime = read_proc_stat().await?;
    let cores = cputime.1.len();

    if cores == 0 {
        return Err(Error::new("/proc/stat reported zero cores"));
    }

    let mut timer = config.interval.timer();

    loop {
        let freqs = read_frequencies().await?;

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

        // Read boost state on intel CPUs
        let boost = boost_status().await.map(|status| match status {
            true => "cpu_boost_on",
            false => "cpu_boost_off",
        });

        let mut values = map!(
            "icon" => Value::icon_progression("cpu", utilization_avg),
            "barchart" => Value::text(barchart),
            "utilization" => Value::percents(utilization_avg * 100.),
            [if !freqs.is_empty()] "frequency" => Value::hertz(freqs.iter().sum::<f64>() / (freqs.len() as f64)),
            [if !freqs.is_empty()] "max_frequency" => Value::hertz(freqs.iter().copied().max_by(f64::total_cmp).unwrap()),
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

        let mut widget = Widget::new().with_format(format.clone());
        widget.set_values(values);
        widget.state = match utilization_avg * 100. {
            x if x > config.critical_cpu => State::Critical,
            x if x > config.warning_cpu => State::Warning,
            x if x > config.info_cpu => State::Info,
            _ => State::Idle,
        };
        api.set_widget(widget)?;

        loop {
            select! {
                _ = timer.tick() => break,
                _ = api.wait_for_update_request() => break,
                Some(action) = actions.recv() => match action.as_ref() {
                    "toggle_format" => {
                        if let Some(ref mut format_alt) = format_alt {
                            std::mem::swap(format_alt, &mut format);
                            break;
                        }
                    }
                    _ => (),
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
        let elapsed = (self.idle + self.non_idle).saturating_sub(old.idle + old.non_idle);
        if elapsed == 0 {
            0.0
        } else {
            ((self.non_idle - old.non_idle) as f64 / elapsed as f64).clamp(0., 1.)
        }
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
        .error("failed to read /proc/stat")?
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

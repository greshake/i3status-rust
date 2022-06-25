//! Display the stats of your NVidia GPU
//!
//! By default `show_temperature` shows the used memory. Clicking the left mouse on the
//! "temperature" part of the block will alternate it between showing used or total available
//! memory.
//!
//! Clicking the left mouse button on the "fan speed" part of the block will cause it to enter into
//! a fan speed setting mode. In this mode you can scroll the mouse wheel over the block to change
//! the fan speeds, and left click to exit the mode.
//!
//! Requires `nvidia-smi` for displaying info and `nvidia_settings` for setting fan speed.
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `gpu_id` | GPU id in system. | `0`
//! `format` | A string to customise the output of this block. See below for available placeholders. | `"$utilization $memory $temperature"`
//! `interval` | Update interval in seconds. | `1`
//! `idle` | Maximum temperature, below which state is set to idle | `50`
//! `good` | Maximum temperature, below which state is set to good | `70`
//! `info` | Maximum temperature, below which state is set to info | `75`
//! `warning` | Maximum temperature, below which state is set to warning | `80`
//!
//! Placeholder   | Type   | Unit
//! --------------|--------|---------------
//! `name`        | Text   | -
//! `utilization` | Number | Percents
//! `memory`      | Number | Bytes
//! `temperature` | Number | Degrees
//! `fan_speed`   | Number | Percents
//! `clocks`      | Number | Hertz
//! `power`       | Number | Watts
//!
//! # Example
//!
//! ```toml
//! [[block]]
//! block = "nvidia_gpu"
//! interval = 1
//! format = "GT 1030 $utilization $temperature $clocks"
//! ```
//!
//! # Icons Used
//! - `gpu`
//!
//! # TODO
//! - Provide a `mappings` option similar to `keyboard_layout`'s  to map GPU names to labels?

use std::process::Stdio;
use std::str::FromStr;

use tokio::io::{BufReader, Lines};
use tokio::process::Command;

const MEM_BTN: usize = 1;
const FAN_BTN: usize = 2;
const QUERY: &str = "--query-gpu=name,memory.total,utilization.gpu,memory.used,temperature.gpu,fan.speed,clocks.current.graphics,power.draw,";
const FORMAT: &str = "--format=csv,noheader,nounits";

use super::prelude::*;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
struct NvidiaGpuConfig {
    format: FormatConfig,
    #[default(1.into())]
    interval: Seconds,
    #[default(0)]
    gpu_id: u64,
    #[default(50)]
    idle: u32,
    #[default(70)]
    good: u32,
    #[default(75)]
    info: u32,
    #[default(80)]
    warning: u32,
}

pub async fn run(config: toml::Value, mut api: CommonApi) -> Result<()> {
    let config = NvidiaGpuConfig::deserialize(config).config_error()?;
    let mut widget = api.new_widget().with_icon("gpu")?.with_format(
        config
            .format
            .with_default("$utilization $memory $temperature")?,
    );

    // Run `nvidia-smi` command
    let mut child = Command::new("nvidia-smi")
        .args(&[
            "-l",
            &config.interval.seconds().to_string(),
            "-i",
            &config.gpu_id.to_string(),
            QUERY,
            FORMAT,
        ])
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .error("Failed to execute nvidia-smi")?;
    let mut reader = BufReader::new(child.stdout.take().unwrap()).lines();

    // Read the initial info
    let mut info = GpuInfo::from_reader(&mut reader).await?;
    let mut show_mem_total = false;
    let mut fan_controlled = false;

    loop {
        let temp_state = match info.temperature {
            t if t <= config.idle => State::Idle,
            t if t <= config.good => State::Good,
            t if t <= config.info => State::Info,
            t if t <= config.warning => State::Warning,
            _ => State::Critical,
        };
        let fan_state = if fan_controlled {
            State::Warning
        } else {
            State::Idle
        };

        widget.set_values(map! {
            "name" => Value::text(info.name.clone()),
            "utilization" => Value::percents(info.utilization),
            "memory" => Value::bytes(if show_mem_total {info.mem_total} else {info.mem_used}).with_instance(MEM_BTN),
            "temperature" => Value::degrees(info.temperature).with_state(temp_state),
            "fan_speed" => Value::percents(info.fan_speed).with_instance(FAN_BTN).with_state(fan_state),
            "clocks" => Value::hertz(info.clocks),
            "power" => Value::watts(info.power_draw),
        });
        api.set_widget(&widget).await?;

        loop {
            select! {
                event = api.event() => match event {
                    UpdateRequest => break,
                    Click(click) => match click.instance {
                        Some(MEM_BTN) if click.button == MouseButton::Left => {
                            show_mem_total = !show_mem_total;
                            break;
                        }
                        Some(FAN_BTN ) => match click.button {
                            MouseButton::Left => {
                                fan_controlled = !fan_controlled;
                                set_fan_speed(config.gpu_id, fan_controlled.then(|| info.fan_speed)).await?;
                                break;
                            }
                            MouseButton::WheelUp if fan_controlled && info.fan_speed < 100 => {
                                info.fan_speed += 1;
                                set_fan_speed(config.gpu_id, Some(info.fan_speed)).await?;
                                break;
                            }
                            MouseButton::WheelDown if fan_controlled && info.fan_speed > 0 => {
                                info.fan_speed -= 1;
                                set_fan_speed(config.gpu_id, Some(info.fan_speed)).await?;
                                break;
                            }
                            _ => (),
                        }
                        _ => (),
                    }
                },
                new_info = GpuInfo::from_reader(&mut reader) => {
                    info = new_info?;
                    break;
                }
                code = child.wait() => {
                    let code = code.error("failed to check nvidia-smi exit code")?;
                    return Err(Error::new(format!("nvidia-smi exited with code {code}")));
                }
            }
        }
    }
}

struct GpuInfo {
    name: String,
    mem_total: f64,   // bytes
    mem_used: f64,    // bytes
    utilization: f64, // percents
    temperature: u32, // degrees
    fan_speed: u32,   // percents
    clocks: f64,      // hertz
    power_draw: f64,  // watts
}

impl GpuInfo {
    /// Read a line from provided reader and parse it
    ///
    /// # Cancel safety
    ///
    /// This method should be cancellation safe, because it has only one `.await` and it is on `next_line`, which is cancellation safe.
    async fn from_reader<B: AsyncBufRead + Unpin>(reader: &mut Lines<B>) -> Result<Self> {
        const ERR_MSG: &str = "failed to read from nvidia-smi";
        reader
            .next_line()
            .await
            .error(ERR_MSG)?
            .error(ERR_MSG)?
            .parse::<GpuInfo>()
            .error("failed to parse nvidia-smi output")
    }
}

impl FromStr for GpuInfo {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        macro_rules! parse {
            ($s:ident -> $($part:ident : $t:ident $(* $mul:expr)?),*) => {{
                let mut parts = $s.trim().split(", ");
                let info = GpuInfo {
                    $(
                    $part: {
                    let $part = parts
                        .next()
                        .or_error(|| format!("missing property: '{}'", stringify!($part)))?
                        .parse::<$t>()
                        .or_error(|| format!("bad property '{}'", stringify!($part)))?;
                    $(let $part = $part * $mul;)?
                    $part
                    },
                    )*
                };
                Ok(info)
            }}
        }
        // `memory` and `clocks` are initially in MB and MHz, so we have to divide them by 1_000_000
        parse!(s -> name: String, mem_total: f64 * 1e-6, utilization: f64, mem_used: f64 * 1e-6, temperature: u32, fan_speed: u32, clocks: f64 * 1e-6, power_draw: f64)
    }
}

async fn set_fan_speed(id: u64, speed: Option<u32>) -> Result<()> {
    const ERR_MSG: &str = "Failed to execute nvidia-settings";
    let mut cmd = Command::new("nvidia-settings");
    if let Some(speed) = speed {
        cmd.args(&[
            "-a",
            &format!("[gpu:{id}]/GPUFanControlState=1"),
            "-a",
            &format!("[fan:{id}]/GPUTargetFanSpeed={speed}"),
        ]);
    } else {
        cmd.args(&["-a", &format!("[gpu:{id}]/GPUFanControlState=0")]);
    }
    if cmd
        .spawn()
        .error(ERR_MSG)?
        .wait()
        .await
        .error(ERR_MSG)?
        .success()
    {
        Ok(())
    } else {
        Err(Error::new(ERR_MSG))
    }
}

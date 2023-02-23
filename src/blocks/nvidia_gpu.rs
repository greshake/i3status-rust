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
//! `format` | A string to customise the output of this block. See below for available placeholders. | `" $icon $utilization $memory $temperature "`
//! `interval` | Update interval in seconds. | `1`
//! `idle` | Maximum temperature, below which state is set to idle | `50`
//! `good` | Maximum temperature, below which state is set to good | `70`
//! `info` | Maximum temperature, below which state is set to info | `75`
//! `warning` | Maximum temperature, below which state is set to warning | `80`
//!
//! Placeholder   | Type   | Unit
//! --------------|--------|---------------
//! `icon`        | Icon   | -
//! `name`        | Text   | -
//! `utilization` | Number | Percents
//! `memory`      | Number | Bytes
//! `temperature` | Number | Degrees
//! `fan_speed`   | Number | Percents
//! `clocks`      | Number | Hertz
//! `power`       | Number | Watts
//!
//! Action                  | Default button
//! ------------------------|----------------
//! `toggle_mem_total`      | Left on `$memory`
//! `toggle_fan_controlled` | Left on `$fan_speed`
//! `fan_speed_up`          | Wheel Up on `$fan_speed`
//! `fan_speed_down`        | Wheel Down on `$fan_speed`
//!
//! # Example
//!
//! ```toml
//! [[block]]
//! block = "nvidia_gpu"
//! interval = 1
//! format = " $icon GT 1030 $utilization $temperature $clocks "
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

const MEM_BTN: &str = "mem_btn";
const FAN_BTN: &str = "fan_btn";
const QUERY: &str = "--query-gpu=name,memory.total,utilization.gpu,memory.used,temperature.gpu,fan.speed,clocks.current.graphics,power.draw,";
const FORMAT: &str = "--format=csv,noheader,nounits";

use super::prelude::*;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(default)]
pub struct Config {
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

pub async fn run(config: Config, mut api: CommonApi) -> Result<()> {
    api.set_default_actions(&[
        (MouseButton::Left, Some(MEM_BTN), "toggle_mem_total"),
        (MouseButton::Left, Some(FAN_BTN), "toggle_fan_controlled"),
        (MouseButton::WheelUp, Some(FAN_BTN), "fan_speed_up"),
        (MouseButton::WheelDown, Some(FAN_BTN), "fan_speed_down"),
    ])
    .await?;

    let mut widget = Widget::new().with_format(
        config
            .format
            .with_default(" $icon $utilization $memory $temperature ")?,
    );

    // Run `nvidia-smi` command
    let mut child = Command::new("nvidia-smi")
        .args([
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
        widget.state = match info.temperature {
            t if t <= config.idle => State::Idle,
            t if t <= config.good => State::Good,
            t if t <= config.info => State::Info,
            t if t <= config.warning => State::Warning,
            _ => State::Critical,
        };

        widget.set_values(map! {
            "icon" => Value::icon(api.get_icon("gpu")?),
            "name" => Value::text(info.name.clone()),
            "utilization" => Value::percents(info.utilization),
            "memory" => Value::bytes(if show_mem_total {info.mem_total} else {info.mem_used}).with_instance(MEM_BTN),
            "temperature" => Value::degrees(info.temperature),
            "fan_speed" => Value::percents(info.fan_speed).with_instance(FAN_BTN).underline(fan_controlled).italic(fan_controlled),
            "clocks" => Value::hertz(info.clocks),
            "power" => Value::watts(info.power_draw),
        });

        api.set_widget(&widget).await?;

        loop {
            select! {
                event = api.event() => match event {
                    UpdateRequest => break,
                    Action(a) if a == "toggle_mem_total" => {
                        show_mem_total = !show_mem_total;
                        break;
                    }
                    Action(a) if a == "toggle_fan_controlled" => {
                        fan_controlled = !fan_controlled;
                        set_fan_speed(config.gpu_id, fan_controlled.then_some(info.fan_speed)).await?;
                        break;
                    }
                    Action(a) if a == "fan_speed_up" && fan_controlled && info.fan_speed < 100 => {
                        info.fan_speed += 1;
                        set_fan_speed(config.gpu_id, Some(info.fan_speed)).await?;
                        break;
                    }
                    Action(a) if a == "fan_speed_down" && fan_controlled && info.fan_speed > 0 => {
                        info.fan_speed -= 1;
                        set_fan_speed(config.gpu_id, Some(info.fan_speed)).await?;
                        break;
                    }
                    _ => (),
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

#[derive(Debug)]
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
                            .error(concat!("missing property: ", stringify!($part)))?
                            .parse::<$t>()
                            .error(concat!("bad property: ", stringify!($part)))?;
                        $(let $part = $part * $mul;)?
                        $part
                    },
                    )*
                };
                Ok(info)
            }}
        }
        // `memory` and `clocks` are initially in MB and MHz, so we have to multiply them by 1_000_000
        parse!(s -> name: String, mem_total: f64 * 1e6, utilization: f64, mem_used: f64 * 1e6, temperature: u32, fan_speed: u32, clocks: f64 * 1e6, power_draw: f64)
    }
}

async fn set_fan_speed(id: u64, speed: Option<u32>) -> Result<()> {
    const ERR_MSG: &str = "Failed to execute nvidia-settings";
    let mut cmd = Command::new("nvidia-settings");
    if let Some(speed) = speed {
        cmd.args([
            "-a",
            &format!("[gpu:{id}]/GPUFanControlState=1"),
            "-a",
            &format!("[fan:{id}]/GPUTargetFanSpeed={speed}"),
        ]);
    } else {
        cmd.args(["-a", &format!("[gpu:{id}]/GPUFanControlState=0")]);
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

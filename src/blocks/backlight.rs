//! A block for displaying the brightness of a backlit device.
//!
//! This module contains the [`Backlight`](./struct.Backlight.html) block, which
//! can display the brightness level of physical backlit devices. Brightness
//! levels are read from and written to the `sysfs` filesystem, so this block
//! does not depend on `xrandr` (and thus it works on Wayland). To set
//! brightness levels using `xrandr`, see the
//! [`Xrandr`](../xrandr/struct.Xrandr.html) block.

use std::fs::OpenOptions;
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use inotify::{EventMask, Inotify, WatchMask};
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::SharedConfig;
use crate::config::{LogicalDirection, Scrolling};
use crate::errors::*;
use crate::formatting::value::Value;
use crate::formatting::FormatTemplate;
use crate::protocol::i3bar_event::{I3BarEvent, MouseButton};
use crate::scheduler::Task;
use crate::subprocess::spawn_child_async;
use crate::widgets::text::TextWidget;
use crate::widgets::I3BarWidget;

/// Read a brightness value from the given path.
fn read_brightness(device_file: &Path) -> Result<u64> {
    let mut file = OpenOptions::new()
        .read(true)
        .open(device_file)
        .block_error("backlight", "Failed to open brightness file")?;
    let mut content = String::new();
    file.read_to_string(&mut content)
        .block_error("backlight", "Failed to read brightness file")?;
    // Removes trailing newline.
    content.pop();
    content
        .parse::<u64>()
        .block_error("backlight", "Failed to read value from brightness file")
}

/// Represents a physical backlit device whose brightness level can be queried.
pub struct BacklitDevice {
    max_brightness: u64,
    device_path: PathBuf,
    root_scaling: f64,
}

/// Clamp scale root to a safe range. Useful values are 1.0 to 3.0.
fn clamp_root_scaling(root_scaling: f64) -> f64 {
    root_scaling.clamp(0.1, 10.0)
}

impl BacklitDevice {
    /// Use the default backlit device, i.e. the first one found in the
    /// `/sys/class/backlight` directory.
    pub fn default(root_scaling: f64) -> Result<Self> {
        let devices = Path::new("/sys/class/backlight")
            .read_dir() // Iterate over entries in the directory.
            .block_error("backlight", "Failed to read backlight device directory")?;

        let first_device = devices
            .take(1)
            .next()
            .block_error("backlight", "No backlit devices found")?
            .block_error("backlight", "Failed to read default device file")?;

        let max_brightness = read_brightness(&first_device.path().join("max_brightness"))?;

        Ok(BacklitDevice {
            max_brightness,
            device_path: first_device.path(),
            root_scaling: clamp_root_scaling(root_scaling),
        })
    }

    /// Use the backlit device `device`. Returns an error if a directory for
    /// that device is not found.
    pub fn from_device(device: String, root_scaling: f64) -> Result<Self> {
        let device_path = Path::new("/sys/class/backlight").join(device);
        if !device_path.exists() {
            return Err(BlockError(
                "backlight".to_string(),
                format!(
                    "Backlight device '{}' does not exist",
                    device_path.to_string_lossy()
                ),
            ));
        }

        let max_brightness = read_brightness(&device_path.join("max_brightness"))?;

        Ok(BacklitDevice {
            max_brightness,
            device_path,
            root_scaling: clamp_root_scaling(root_scaling),
        })
    }

    /// Query the brightness value for this backlit device, as a percent.
    pub fn brightness(&self) -> Result<u64> {
        let raw = read_brightness(&self.brightness_file())?;
        let brightness_ratio =
            (raw as f64 / self.max_brightness as f64).powf(self.root_scaling.recip());
        let brightness = (brightness_ratio * 100.0).round() as u64;
        match brightness {
            0..=100 => Ok(brightness),
            _ => Ok(100),
        }
    }

    /// Set the brightness value for this backlit device, as a percent.
    pub fn set_brightness(&self, value: u64) -> Result<()> {
        let safe_value = match value {
            0..=100 => value,
            _ => 100,
        };
        let ratio = (safe_value as f64 / 100.0).powf(self.root_scaling);
        let raw = std::cmp::max(1, (ratio * (self.max_brightness as f64)).round() as u64);

        let file = OpenOptions::new()
            .write(true)
            .open(self.device_path.join("brightness"));
        if file.is_err() {
            // TODO: Find a way to issue a non-fatal error, since this is likely
            // due to a permissions issue and not the fault of the user. It
            // should not crash the bar.
            // Error: "Failed to open brightness file for writing"
            return self.set_brightness_via_dbus(raw);
        }

        // It's safe to unwrap() here because we checked for errors above.
        file.unwrap()
            .write_fmt(format_args!("{}", raw))
            .block_error("backlight", "Failed to write into brightness file")
    }

    fn set_brightness_via_dbus(&self, raw_value: u64) -> Result<()> {
        let device_name = self
            .device_path
            .file_name()
            .and_then(|x| x.to_str())
            .block_error("backlight", "Malformed device path")?;

        let con = dbus::ffidisp::Connection::get_private(dbus::ffidisp::BusType::System)
            .block_error("backlight", "Failed to establish D-Bus connection.")?;
        let msg = dbus::Message::new_method_call(
            "org.freedesktop.login1",
            "/org/freedesktop/login1/session/auto",
            "org.freedesktop.login1.Session",
            "SetBrightness",
        )
        .block_error("backlight", "Failed to create D-Bus message")?
        .append2("backlight", device_name)
        .append1(raw_value as u32);

        con.send_with_reply_and_block(msg, 1000)
            .block_error("backlight", "Failed to send D-Bus message")
            .map(|_| ())
    }

    /// The brightness file itself.
    // amdgpu drivers set the actual_brightness in a different scale than [0, max_brightness],
    // so we have to use the 'brightness' file instead. This may be fixed in the new 5.7 kernel?
    pub fn brightness_file(&self) -> PathBuf {
        if self.device_path.ends_with("amdgpu_bl0") {
            self.device_path.join("brightness")
        } else {
            self.device_path.join("actual_brightness")
        }
    }
}

/// A block for displaying the brightness of a backlit device.
pub struct Backlight {
    id: usize,
    output: TextWidget,
    device: BacklitDevice,
    step_width: u64,
    minimum: u64,
    maximum: u64,
    cycle: Vec<u64>,
    cycle_index: usize,
    scrolling: Scrolling,
    invert_icons: bool,
    on_click: Option<String>,
    format: FormatTemplate,
}

/// Configuration for the [`Backlight`](./struct.Backlight.html) block.
#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct BacklightConfig {
    /// The backlight device in `/sys/class/backlight/` to read brightness from.
    pub device: Option<String>,

    /// The steps brightness is in/decreased for the selected screen (When greater than 50 it gets limited to 50)
    pub step_width: u64,

    /// the min and max brightness limit the range over which the brightness can be in/decreased
    pub minimum: u64,
    pub maximum: u64,

    /// when the block is clicked, brightness cycles through all of these
    pub cycle: Option<Vec<u64>>,

    /// Format string for displaying backlight information.
    /// placeholders: {brightness}
    pub format: FormatTemplate,

    /// Scaling exponent reciprocal (ie. root). Some devices expose raw values
    /// that are best handled with nonlinear scaling. The human perception of
    /// lightness is close to the cube root of relative luminance. Settings
    /// between 2.4 and 3.0 are worth trying.
    /// More information: <https://en.wikipedia.org/wiki/Lightness>
    ///
    /// For devices with few discrete steps this should be 1.0 (linear).
    pub root_scaling: f64,

    pub invert_icons: bool,

    pub on_click: Option<String>,
}

impl Default for BacklightConfig {
    fn default() -> Self {
        Self {
            device: None,
            step_width: 5,
            root_scaling: 1f64,
            invert_icons: false,
            on_click: None,
            format: FormatTemplate::default(),
            minimum: 5,
            maximum: 100,
            cycle: None,
        }
    }
}

impl Backlight {
    fn advance_cycle(&mut self) -> Result<()> {
        if self.cycle.is_empty() {
            return Ok(());
        }
        let current = self.device.brightness()?;
        let nearest = if self.cycle[self.cycle_index] == current {
            self.cycle_index // shortcut
        } else {
            let current = current as i64;
            // by default, restart cycle at nearest value
            let key = |idx: usize, val: i64| {
                // distance to current brightness is the first criterion
                let distance = (val - current).abs();
                // offset makes it so that in case of an equality for distance,
                // the winning index is the first one after cycle_index (circularly)
                let offset = if idx >= self.cycle_index {
                    0
                } else {
                    self.cycle.len()
                };
                (distance, idx + offset)
            };
            self.cycle
                .iter()
                .enumerate()
                .min_by_key(|&(idx, &val)| key(idx, val as i64))
                .unwrap() // cycle has been checked non-empty
                .0
        };
        self.cycle_index = (nearest + 1) % self.cycle.len();
        self.device.set_brightness(self.cycle[self.cycle_index])
    }
}

impl ConfigBlock for Backlight {
    type Config = BacklightConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        shared_config: SharedConfig,
        tx_update_request: Sender<Task>,
    ) -> Result<Self> {
        let device = match block_config.device {
            Some(path) => BacklitDevice::from_device(path, block_config.root_scaling),
            None => BacklitDevice::default(block_config.root_scaling),
        }?;
        let brightness_file = device.brightness_file();

        let (minimum, maximum) = if block_config.minimum <= block_config.maximum {
            (block_config.minimum, block_config.maximum)
        } else {
            (block_config.maximum, block_config.minimum)
        };

        let backlight = Self {
            id,
            device,
            step_width: block_config.step_width,
            minimum,
            maximum,
            cycle: block_config.cycle.unwrap_or_else(|| vec![minimum, maximum]),
            cycle_index: 0,
            on_click: block_config.on_click,
            scrolling: shared_config.scrolling,
            output: TextWidget::new(id, 0, shared_config),
            invert_icons: block_config.invert_icons,
            format: block_config.format.with_default("{brightness}")?,
        };

        // Spin up a thread to watch for changes to the brightness file for the
        // device, and schedule an update if needed.
        thread::Builder::new()
            .name("backlight".into())
            .spawn(move || {
                let mut notify = Inotify::init().expect("Failed to start inotify");
                notify
                    .add_watch(brightness_file, WatchMask::MODIFY)
                    .expect("Failed to watch brightness file");

                let mut buffer = [0; 1024];
                loop {
                    let mut events = notify
                        .read_events_blocking(&mut buffer)
                        .expect("Error while reading inotify events");

                    if events.any(|event| event.mask.contains(EventMask::MODIFY)) {
                        tx_update_request
                            .send(Task {
                                id,
                                update_time: Instant::now(),
                            })
                            .unwrap();
                    }

                    // Avoid update spam.
                    thread::sleep(Duration::from_millis(250))
                }
            })
            .unwrap();

        Ok(backlight)
    }

    fn override_on_click(&mut self) -> Option<&mut Option<String>> {
        Some(&mut self.on_click)
    }
}

impl Block for Backlight {
    fn update(&mut self) -> Result<Option<Update>> {
        let mut brightness = self.device.brightness()?;
        let values = map!(
            "brightness" => Value::from_integer(brightness as i64).percents(),
        );
        let texts = self.format.render(&values)?;
        self.output.set_texts(texts);
        if self.invert_icons {
            brightness = 100 - brightness;
        }
        self.output.set_icon(match brightness {
            0..=6 => "backlight_empty",
            7..=13 => "backlight_1",
            14..=20 => "backlight_2",
            21..=26 => "backlight_3",
            27..=33 => "backlight_4",
            34..=40 => "backlight_5",
            41..=46 => "backlight_6",
            47..=53 => "backlight_7",
            54..=60 => "backlight_8",
            61..=67 => "backlight_9",
            68..=73 => "backlight_10",
            74..=80 => "backlight_11",
            81..=87 => "backlight_12",
            88..=93 => "backlight_13",
            _ => "backlight_full",
        })?;

        Ok(None)
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.output]
    }

    fn click(&mut self, event: &I3BarEvent) -> Result<()> {
        match event.button {
            MouseButton::Right => self.advance_cycle()?,
            MouseButton::Left => {
                if let Some(ref cmd) = self.on_click {
                    spawn_child_async("sh", &["-c", cmd])
                        .block_error("backlight", "could not spawn child")?
                } else {
                    self.advance_cycle()?
                }
            }
            _ => {
                let brightness = self.device.brightness()? as i64;
                let step_width = self.step_width as i64;
                if let Some(direction) = self.scrolling.to_logical_direction(event.button) {
                    use LogicalDirection::*;
                    let sign = match direction {
                        Up => 1,
                        Down => -1,
                    };
                    self.device.set_brightness(
                        (brightness + sign * step_width)
                            .clamp(self.minimum as i64, self.maximum as i64)
                            as u64,
                    )?
                }
            }
        }
        Ok(())
    }

    fn id(&self) -> usize {
        self.id
    }
}

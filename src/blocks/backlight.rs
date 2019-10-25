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
use uuid::Uuid;

use crate::blocks::{Block, ConfigBlock};
use crate::config::Config;
use crate::errors::*;
use crate::input::{I3BarEvent, MouseButton};
use crate::scheduler::Task;
use crate::widget::I3BarWidget;
use crate::widgets::button::ButtonWidget;

/// Read a brightness value from the given path.
fn read_brightness(device_file: &Path) -> Result<u64> {
    let mut file = OpenOptions::new()
            .read(true)
            .open(device_file)
            .block_error("backlight", "Failed to open brightness file")?;
    let mut content = String::new();
    file.read_to_string(&mut content).block_error(
        "backlight",
        "Failed to read brightness file",
    )?;
    // Removes trailing newline.
    content.pop();
    content.parse::<u64>().block_error(
        "backlight",
        "Failed to read value from brightness file",
    )
}

/// Represents a physical backlit device whose brightness level can be queried.
pub struct BacklitDevice {
    max_brightness: u64,
    device_path: PathBuf,
}

impl BacklitDevice {
    /// Use the default backlit device, i.e. the first one found in the
    /// `/sys/class/backlight` directory.
    pub fn default() -> Result<Self> {
        let devices = Path::new("/sys/class/backlight")
                           .read_dir() // Iterate over entries in the directory.
                           .block_error("backlight",
                                        "Failed to read backlight device directory")?;

        let first_device = match devices.take(1).next() {
            None => Err(BlockError(
                "backlight".to_string(),
                "No backlit devices found".to_string(),
            )),
            Some(device) => {
                device.map_err(|_| {
                    BlockError(
                        "backlight".to_string(),
                        "Failed to read default device file".to_string(),
                    )
                })
            }
        }?;

        let max_brightness = read_brightness(&first_device.path().join("max_brightness"))?;

        Ok(BacklitDevice {
            max_brightness,
            device_path: first_device.path(),
        })
    }

    /// Use the backlit device `device`. Returns an error if a directory for
    /// that device is not found.
    pub fn from_device(device: String) -> Result<Self> {
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
        })
    }

    /// Query the brightness value for this backlit device, as a percent.
    pub fn brightness(&self) -> Result<u64> {
        let raw = read_brightness(&self.brightness_file())?;
        let brightness = ((raw as f64 / self.max_brightness as f64) * 100.0).round() as u64;
        match brightness {
            0..=100 => Ok(brightness),
            _ => Ok(100),
        }
    }

    /// Set the brightness value for this backlit device, as a percent.
    pub fn set_brightness(&self, value: u64) -> Result<()> {
        let file = OpenOptions::new().write(true).open(self.device_path.join(
            "brightness",
        ));
        if file.is_err() {
            // TODO: Find a way to issue a non-fatal error, since this is likely
            // due to a permissions issue and not the fault of the user. It
            // should not crash the bar.
            // Error: "Failed to open brightness file for writing"
            return Ok(());
        }
        let safe_value = match value {
            0..=100 => value,
            _ => 100,
        };
        let raw = (((safe_value as f64) / 100.0) * (self.max_brightness as f64)).round() as u64;
        // It's safe to unwrap() here because we checked for errors above.
        file.unwrap()
            .write_fmt(format_args!("{}", raw))
            .block_error("backlight", "Failed to write into brightness file")
    }

    /// The brightness file itself.
    pub fn brightness_file(&self) -> PathBuf {
        self.device_path.join("brightness")
    }
}

/// A block for displaying the brightness of a backlit device.
pub struct Backlight {
    id: String,
    output: ButtonWidget,
    device: BacklitDevice,
    step_width: u64,
}

/// Configuration for the [`Backlight`](./struct.Backlight.html) block.
#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct BacklightConfig {
    /// The backlight device in `/sys/class/backlight/` to read brightness from.
    #[serde(default = "BacklightConfig::default_device")]
    pub device: Option<String>,

    /// The steps brightness is in/decreased for the selected screen (When greater than 50 it gets limited to 50)
    #[serde(default = "BacklightConfig::default_step_width")]
    pub step_width: u64,
}

impl BacklightConfig {
    fn default_device() -> Option<String> {
        None
    }

    fn default_step_width() -> u64 {
        5
    }
}

impl ConfigBlock for Backlight {
    type Config = BacklightConfig;

    fn new(block_config: Self::Config, config: Config, tx_update_request: Sender<Task>) -> Result<Self> {
        let device = match block_config.device {
            Some(path) => BacklitDevice::from_device(path),
            None => BacklitDevice::default(),
        }?;

        let id = Uuid::new_v4().simple().to_string();
        let brightness_file = device.brightness_file();

        let backlight = Backlight {
            output: ButtonWidget::new(config, &id),
            id: id.clone(),
            device,
            step_width: block_config.step_width,
        };

        // Spin up a thread to watch for changes to the brightness file for the
        // device, and schedule an update if needed.
        thread::spawn(move || {
            let mut notify = Inotify::init().expect("Failed to start inotify");
            notify
                .add_watch(brightness_file, WatchMask::MODIFY)
                .expect("Failed to watch brightness file");

            let mut buffer = [0; 1024];
            loop {
                let mut events = notify.read_events_blocking(&mut buffer).expect(
                    "Error while reading inotify events",
                );

                if events.any(|event| event.mask.contains(EventMask::MODIFY)) {
                    tx_update_request.send(Task {
                        id: id.clone(),
                        update_time: Instant::now(),
                    }).unwrap();
                }

                // Avoid update spam.
                thread::sleep(Duration::from_millis(250))
            }
        });

        Ok(backlight)
    }
}

impl Block for Backlight {
    fn update(&mut self) -> Result<Option<Duration>> {
        let brightness = self.device.brightness()?;
        self.output.set_text(format!("{}%", brightness));
        match brightness {
            0..=19 => self.output.set_icon("backlight_empty"),
            20..=39 => self.output.set_icon("backlight_partial1"),
            40..=59 => self.output.set_icon("backlight_partial2"),
            60..=79 => self.output.set_icon("backlight_partial3"),
            _ => self.output.set_icon("backlight_full"),
        }
        Ok(None)
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.output]
    }

    fn click(&mut self, event: &I3BarEvent) -> Result<()> {
        if let Some(ref name) = event.name {
            if name.as_str() == self.id {
                let brightness = self.device.brightness()?;
                match event.button {
                    MouseButton::WheelUp => {
                        if brightness < 100 {
                            self.device.set_brightness(brightness + self.step_width)?;
                        }
                    }
                    MouseButton::WheelDown => {
                        if brightness > self.step_width {
                            self.device.set_brightness(brightness - self.step_width)?;
                        }
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }

    fn id(&self) -> &str {
        &self.id
    }
}

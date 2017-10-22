use std::fs::OpenOptions;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;
use chan::Sender;

use block::{Block, ConfigBlock};
use config::Config;
use de::deserialize_duration;
use errors::*;
use widgets::text::TextWidget;
use widget::I3BarWidget;
use input::I3BarEvent;
use scheduler::Task;

use uuid::Uuid;

/// Read a brightness value from the given path.
fn read_brightness(device_file: &Path) -> Result<u64> {
    let mut file = try!(OpenOptions::new()
        .read(true)
        .open(device_file)
        .block_error("backlight",
                     "Failed to open brightness file"));
    let mut content = String::new();
    try!(file.read_to_string(&mut content)
         .block_error("backlight",
                      "Failed to read brightness file"));
    // Removes trailing newline.
    content.pop();
    content.parse::<u64>()
        .block_error("backlight",
                     "Failed to read value from brightness file")
}

pub struct BacklitDevice {
    pub max_brightness: u64,
    device_path: PathBuf,
}

impl BacklitDevice {
    /// Use the default backlit device, i.e. the first one found in the
    /// `/sys/class/backlight` directory.
    pub fn default() -> Result<Self> {
        let devices = try!(Path::new("/sys/class/backlight")
                           .read_dir() // Iterate over entries in the directory.
                           .block_error("backlight",
                                        "Failed to read backlight device directory"));

        let first_device = try!(match devices.take(1).next() {
            None => Err(BlockError("backlight".to_string(),
                                   "No backlit devices found".to_string())),
            Some(device) => device.map_err(|_| {
                BlockError("backlight".to_string(),
                           "Failed to read default device file".to_string())
            }),
        });

        let max_brightness = try!(read_brightness(&first_device.path()
                                                  .join("max_brightness")));

        Ok(BacklitDevice {
            max_brightness: max_brightness,
            device_path: first_device.path()
        })
    }

    /// Use the backlit device `device`. Raises an error if a directory for that
    /// device is not found.
    pub fn from_device(device: String) -> Result<Self> {
        let device_path = Path::new("/sys/class/backlight").join(device);
        if !device_path.exists() {
            return Err(BlockError("backlight".to_string(),
                                  format!("Backlight device '{}' does not exist",
                                          device_path.to_string_lossy())));
        }

        let max_brightness = try!(read_brightness(&device_path
                                                  .join("max_brightness")));

        Ok(BacklitDevice {
            max_brightness: max_brightness,
            device_path: device_path,
        })
    }

    /// Query the brightness value for this backlit device.
    pub fn brightness(&self) -> Result<u64> {
        read_brightness(&self.device_path.join("brightness"))
    }
}

pub struct Backlight {
    id: String,
    update_interval: Duration,
    output: TextWidget,
    device: BacklitDevice,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct BacklightConfig {
    /// The update interval, in seconds.
    #[serde(default = "BacklightConfig::default_interval", deserialize_with = "deserialize_duration")]
    pub interval: Duration,

    /// The backlight device in `/sys/class/backlight/` to read brightness from.
    #[serde(default = "BacklightConfig::default_device")]
    pub device: Option<String>,
}

impl BacklightConfig {
    fn default_interval() -> Duration {
        Duration::from_secs(5)
    }

    fn default_device() -> Option<String> {
        None
    }
}

impl ConfigBlock for Backlight {
    type Config = BacklightConfig;

    fn new(block_config: Self::Config, config: Config, _tx_update_request: Sender<Task>) -> Result<Self> {
        let device = try!(match block_config.device {
            Some(path) => BacklitDevice::from_device(path),
            None => BacklitDevice::default(),
        });

        let mut backlight = Backlight {
            output: TextWidget::new(config),
            id: Uuid::new_v4().simple().to_string(),
            update_interval: block_config.interval,
            device: device,
        };

        backlight.output.set_icon("xrandr");
        Ok(backlight)
    }
}

impl Block for Backlight {
    fn update(&mut self) -> Result<Option<Duration>> {
        let brightness = try!(self.device.brightness());
        let display = ((brightness as f64 / self.device.max_brightness as f64) * 100.0) as u64;
        self.output.set_text(format!("{}%", display));
        Ok(Some(self.update_interval))
    }

    fn view(&self) -> Vec<&I3BarWidget> {
        vec![&self.output]
    }

    fn click(&mut self, _: &I3BarEvent) -> Result<()> {
        Ok(())
    }

    fn id(&self) -> &str {
        &self.id
    }
}

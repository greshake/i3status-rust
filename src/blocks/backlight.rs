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

pub struct Backlight {
    output: TextWidget,
    id: String,
    update_interval: Duration,
    max_brightness: u64,
    device_path: PathBuf,
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

fn get_default_device() -> Result<PathBuf> {
    let devices = try!(Path::new("/sys/class/backlight")
        .read_dir()
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

   Ok(first_device.path())
}

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

impl ConfigBlock for Backlight {
    type Config = BacklightConfig;

    fn new(block_config: Self::Config, config: Config, _tx_update_request: Sender<Task>) -> Result<Self> {
        let mut backlight = Backlight {
            output: TextWidget::new(config),
            id: Uuid::new_v4().simple().to_string(),
            update_interval: block_config.interval,
            max_brightness: 0,
            device_path: match block_config.device {
                Some(path) => Path::new("/sys/class/backlight").join(path),
                None => try!(get_default_device()),
            },
        };

        if !backlight.device_path.exists() {
            return Err(BlockError("backlight".to_string(),
                                  format!("Backlight device '{}' does not exist",
                                          backlight.device_path.to_string_lossy())));
        }

        backlight.max_brightness = try!(read_brightness(
            &backlight.device_path.join("max_brightness")
        ));

        Ok(backlight)
    }
}

impl Block for Backlight {
    fn update(&mut self) -> Result<Option<Duration>> {
        let brightness = try!(read_brightness(&self.device_path.join("brightness")));
        let display = ((brightness as f64 / self.max_brightness as f64) * 100.0) as u64;
        self.output.set_text(format!("{}%", display));
        self.output.set_icon("xrandr");
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

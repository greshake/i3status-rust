//! The brightness of a backlight device
//!
//! This block reads brightness information directly from the filesystem, so it works under both
//! X11 and Wayland. The block uses `inotify` to listen for changes in the device's brightness
//! directly, so there is no need to set an update interval. This block uses DBus to set brightness
//! level using the mouse wheel, but will [fallback to sysfs](#d-bus-fallback) if `systemd-logind` is not used.
//!
//! # Root scaling
//!
//! Some devices expose raw values that are best handled with nonlinear scaling. The human perception of lightness is close to the cube root of relative luminance, so settings for `root_scaling` between 2.4 and 3.0 are worth trying. For devices with few discrete steps this should be 1.0 (linear). More information: <https://en.wikipedia.org/wiki/Lightness>
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `device` | The `/sys/class/backlight` device to read brightness information from.  When there is no `device` specified, this block will display information from the first device found in the `/sys/class/backlight` directory. If you only have one display, this approach should find it correctly.| Default device
//! `format` | A string to customise the output of this block. See below for available placeholders. | `" $icon $brightness "`
//! `step_width` | The brightness increment to use when scrolling, in percent | `5`
//! `minimum` | The minimum brightness that can be scrolled down to | `5`
//! `maximum` | The maximum brightness that can be scrolled up to | `100`
//! `cycle` | The brightnesses to cycle through on each click | `[minimum, maximum]`
//! `root_scaling` | Scaling exponent reciprocal (ie. root) | `1.0`
//! `invert_icons` | Invert icons' ordering, useful if you have colorful emoji | `false`
//! `ddcci_sleep_multiplier` | [See ddcutil documentation](https://www.ddcutil.com/performance_options/#option-sleep-multiplier) | `1.0`
//! `ddcci_max_tries_write_read` | The maximum number of times to attempt writing to  or reading from a ddcci monitor | `10`
//!
//! Placeholder  | Value                                     | Type   | Unit
//! -------------|-------------------------------------------|--------|---------------
//! `icon`       | Icon based on backlight's state           | Icon   | -
//! `brightness` | Current brightness                        | Number | %
//!
//! Action            | Default button
//! ------------------|---------------
//! `cycle`           | Left
//! `brightness_up`   | Wheel Up
//! `brightness_down` | Wheel Down
//!
//! # Example
//!
//! ```toml
//! [[block]]
//! block = "backlight"
//! device = "intel_backlight"
//! ```
//!
//! # D-Bus Fallback
//!
//! If you don't use `systemd-logind` i3status-rust will attempt to set the brightness
//! using sysfs. In order to do this you'll need to have write permission.
//! You can do this by writing a `udev` rule for your system.
//!
//! First, check that your user is a member of the "video" group using the
//! `groups` command. Then add a rule in the `/etc/udev/rules.d/` directory
//! containing the following, for example in `backlight.rules`:
//!
//! ```
//! ACTION=="add", SUBSYSTEM=="backlight", GROUP="video", MODE="0664"
//! ```
//!
//! This will allow the video group to modify all backlight devices. You will
//! also need to restart for this rule to take effect.
//!
//! # Icons Used
//! - `backlight` (as a progression)

use std::cmp::max;
use std::ops::Range;
use std::path::{Path, PathBuf};

use inotify::{Inotify, WatchMask};
use tokio::fs::{read_dir, OpenOptions};
use tokio::io::AsyncWriteExt;

use super::prelude::*;
use crate::util::read_file;

make_log_macro!(debug, "backlight");

#[zbus::dbus_proxy(
    interface = "org.freedesktop.login1.Session",
    default_service = "org.freedesktop.login1",
    default_path = "/org/freedesktop/login1/session/auto"
)]
trait Session {
    fn set_brightness(&self, subsystem: &str, name: &str, brightness: u32) -> zbus::Result<()>;
}

/// Location of backlight devices
const DEVICES_PATH: &str = "/sys/class/backlight";

/// Filename for device's max brightness
const FILE_MAX_BRIGHTNESS: &str = "max_brightness";

/// Filename for current brightness.
const FILE_BRIGHTNESS: &str = "actual_brightness";

/// amdgpu drivers set the actual_brightness in a different scale than
/// [0, max_brightness], so we have to use the 'brightness' file instead.
/// This may be fixed in the new 5.7 kernel?
const FILE_BRIGHTNESS_AMD: &str = "brightness";

/// set the requested brightness level
const FILE_BRIGHTNESS_WRITE: &str = "brightness";

/// Range of valid values for `root_scaling`
const ROOT_SCALDING_RANGE: Range<f64> = 0.1..10.;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(default)]
pub struct Config {
    device: Option<String>,
    format: FormatConfig,
    #[default(5)]
    step_width: u8,
    #[default(5)]
    minimum: u8,
    #[default(100)]
    maximum: u8,
    cycle: Option<Vec<u8>>,
    #[default(1.0)]
    root_scaling: f64,
    invert_icons: bool,
    #[default(1.0)]
    ddcci_sleep_multiplier: f64,
    #[default(10)]
    ddcci_max_tries_write_read: u8,
}

pub async fn run(config: Config, mut api: CommonApi) -> Result<()> {
    api.set_default_actions(&[
        (MouseButton::Left, None, "cycle"),
        (MouseButton::WheelUp, None, "brightness_up"),
        (MouseButton::WheelDown, None, "brightness_down"),
    ])
    .await?;

    let mut widget = Widget::new().with_format(config.format.with_default(" $icon $brightness ")?);

    let mut cycle = config
        .cycle
        .unwrap_or_else(|| vec![config.minimum, config.maximum])
        .into_iter()
        .cycle();

    let device = match &config.device {
        None => {
            BacklightDevice::default(
                config.root_scaling,
                config.ddcci_sleep_multiplier,
                config.ddcci_max_tries_write_read,
            )
            .await?
        }
        Some(path) => {
            BacklightDevice::from_device(
                path,
                config.root_scaling,
                config.ddcci_sleep_multiplier,
                config.ddcci_max_tries_write_read,
            )
            .await?
        }
    };

    // Watch for brightness changes
    let mut notify = Inotify::init().error("Failed to start inotify")?;
    notify
        .add_watch(&device.read_brightness_file, WatchMask::MODIFY)
        .error("Failed to watch brightness file")?;
    let mut file_changes = notify
        .event_stream([0; 1024])
        .error("Failed to create event stream")?;

    loop {
        let brightness = api.recoverable(|| device.brightness()).await?;

        let mut icon_value = brightness as f64 / 100.0;
        if config.invert_icons {
            icon_value = 1.0 - icon_value;
        }

        widget.set_values(map! {
            "icon" => Value::icon(api.get_icon_in_progression("backlight", icon_value)?),
            "brightness" => Value::percents(brightness)
        });
        api.set_widget(&widget).await?;

        loop {
            select! {
                _ = file_changes.next() => break,
                event = api.event() => match event {
                    Action(a) if a == "cycle" => {
                        if let Some(brightness) = cycle.next() {
                            device.set_brightness(brightness).await?;
                        }
                    }
                    Action(a) if a == "brightness_up" => {
                        device.set_brightness(
                            (brightness + config.step_width) .clamp(config.minimum, config.maximum)
                        ).await?;
                    }
                    Action(a) if a == "brightness_down" => {
                        device.set_brightness(
                            brightness
                                .saturating_sub(config.step_width)
                                .clamp(config.minimum, config.maximum)
                        ).await?;
                    }
                    _ => (),
                }
            }
        }
    }
}

/// Read a brightness value from the given path.
async fn read_brightness_raw(
    device_file: &Path,
    ddcci_sleep_multiplier: f64,
    ddcci_max_tries_write_read: u8,
) -> Result<u64> {
    let val = match read_file(device_file).await {
        Ok(v) => v,
        Err(_) => 'blk: {
            for i in 1..ddcci_max_tries_write_read {
                debug!("retry {i} reading brightness");
                // See https://glenwing.github.io/docs/VESA-DDCCI-1.1.pdf
                // Section 4.3 for timing explanation
                sleep(Duration::from_millis(
                    (40.0 * ddcci_sleep_multiplier).round() as u64,
                ))
                .await;
                if let Ok(val) = read_file(device_file).await {
                    break 'blk val;
                }
            }
            return Err(Error::new(
                "Failed to read brightness file, check your ddcci settings",
            ));
        }
    };
    val.parse()
        .error("Failed to read value from brightness file")
}

/// Represents a physical backlight device whose brightness level can be queried.
struct BacklightDevice {
    device_name: String,
    read_brightness_file: PathBuf,
    write_brightness_file: PathBuf,
    max_brightness: u64,
    root_scaling: f64,
    dbus_proxy: SessionProxy<'static>,
    ddcci_sleep_multiplier: f64,
    ddcci_max_tries_write_read: u8,
}

impl BacklightDevice {
    async fn new(
        device_path: PathBuf,
        root_scaling: f64,
        ddcci_sleep_multiplier: f64,
        ddcci_max_tries_write_read: u8,
    ) -> Result<Self> {
        let dbus_conn = new_system_dbus_connection().await?;
        Ok(Self {
            read_brightness_file: device_path.join({
                if device_path.ends_with("amdgpu_bl0") {
                    FILE_BRIGHTNESS_AMD
                } else {
                    FILE_BRIGHTNESS
                }
            }),
            write_brightness_file: device_path.join(FILE_BRIGHTNESS_WRITE),
            device_name: device_path
                .file_name()
                .map(|x| x.to_str().unwrap().into())
                .error("Malformed device path")?,
            max_brightness: read_brightness_raw(
                &device_path.join(FILE_MAX_BRIGHTNESS),
                ddcci_sleep_multiplier,
                ddcci_max_tries_write_read,
            )
            .await?,
            root_scaling: root_scaling.clamp(ROOT_SCALDING_RANGE.start, ROOT_SCALDING_RANGE.end),
            dbus_proxy: SessionProxy::new(&dbus_conn)
                .await
                .error("failed to create SessionProxy")?,
            ddcci_sleep_multiplier,
            ddcci_max_tries_write_read,
        })
    }

    /// Use the default backlight device, i.e. the first one found in the
    /// `/sys/class/backlight` directory.
    async fn default(
        root_scaling: f64,
        ddcci_sleep_multiplier: f64,
        ddcci_max_tries_write_read: u8,
    ) -> Result<Self> {
        let device = read_dir(DEVICES_PATH)
            .await
            .error("Failed to read backlight device directory")?
            .next_entry()
            .await
            .error("No backlight devices found")?
            .error("Failed to read default device file")?;
        Self::new(
            device.path(),
            root_scaling,
            ddcci_sleep_multiplier,
            ddcci_max_tries_write_read,
        )
        .await
    }

    /// Use the backlight device `device`. Returns an error if a directory for
    /// that device is not found.
    async fn from_device(
        device: &str,
        root_scaling: f64,
        ddcci_sleep_multiplier: f64,
        ddcci_max_tries_write_read: u8,
    ) -> Result<Self> {
        Self::new(
            Path::new(DEVICES_PATH).join(device),
            root_scaling,
            ddcci_sleep_multiplier,
            ddcci_max_tries_write_read,
        )
        .await
    }

    /// Query the brightness value for this backlight device, as a percent.
    async fn brightness(&self) -> Result<u8> {
        let raw = read_brightness_raw(
            &self.read_brightness_file,
            self.ddcci_sleep_multiplier,
            self.ddcci_max_tries_write_read,
        )
        .await?;

        let brightness_ratio =
            (raw as f64 / self.max_brightness as f64).powf(self.root_scaling.recip());

        ((brightness_ratio * 100.0).round() as i64)
            .try_into()
            .ok()
            .filter(|brightness| (0..=100).contains(brightness))
            .error("Brightness is not in [0, 100]")
    }

    /// Set the brightness value for this backlight device, as a percent.
    async fn set_brightness(&self, value: u8) -> Result<()> {
        let value = value.clamp(0, 100);
        let ratio = (value as f64 / 100.0).powf(self.root_scaling);
        let raw = max(1, (ratio * (self.max_brightness as f64)).round() as u32);
        match self
            .dbus_proxy
            .set_brightness("backlight", &self.device_name, raw)
            .await
        {
            Ok(()) => Ok(()),
            Err(_) => {
                // Fall back to writing to sysfs brightness file
                let mut file = OpenOptions::new()
                    .write(true)
                    .truncate(true)
                    .open(&self.write_brightness_file)
                    .await
                    .error("Could not open brightness file to write")?;
                file.write_all(raw.to_string().as_bytes())
                    .await
                    .error("Could not write sysfs brightness")
            }
        }
    }
}

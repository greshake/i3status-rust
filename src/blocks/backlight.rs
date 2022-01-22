//! The brightness of a backlight device
//!
//! This block reads brightness information directly from the filesystem, so it works under both
//! X11 and Wayland. The block uses `inotify` to listen for changes in the device's brightness
//! directly, so there is no need to set an update interval. This block uses DBus to set brightness
//! level using the mouse wheel.
//!
//! # Root scaling
//!
//! Some devices expose raw values that are best handled with nonlinear scaling. The human perception of lightness is close to the cube root of relative luminance, so settings for `root_scaling` between 2.4 and 3.0 are worth trying. For devices with few discrete steps this should be 1.0 (linear). More information: <https://en.wikipedia.org/wiki/Lightness>
//!
//! # Configuration
//!
//! Key | Values | Required | Default
//! ----|--------|----------|--------
//! `device` | The `/sys/class/backlight` device to read brightness information from.  When there is no `device` specified, this block will display information from the first device found in the `/sys/class/backlight` directory. If you only have one display, this approach should find it correctly.| No | Default device
//! `format` | A string to customise the output of this block. See below for available placeholders. | No | `"$brightness"`
//! `step_width` | The brightness increment to use when scrolling, in percent | No | `5`
//! `minimum` | The minimum brightness that can be scrolled down to | No | `1`
//! `maximum` | The maximum brightness that can be scrolled up to | No | `100`
//! `cycle` | The brightnesses to cycle through on each click | No | `[minimum, maximum]`
//! `root_scaling` | Scaling exponent reciprocal (ie. root) | No | `1.0`
//! `invert_icons` | Invert icons' ordering, useful if you have colorful emoji | No | `false`
//!
//! Placeholder  | Value              | Type   | Unit
//! -------------|--------------------|--------|---------------
//! `brightness` | Current brightness | Number | %
//!
//! # Example
//!
//! ```toml
//! [[block]]
//! block = "backlight"
//! device = "intel_backlight"
//! ```
//!
//! # Icons Used
//! - `backlight_empty` (when brightness between 0 and 6%)
//! - `backlight_1` (when brightness between 7 and 13%)
//! - `backlight_2` (when brightness between 14 and 20%)
//! - `backlight_3` (when brightness between 21 and 26%)
//! - `backlight_4` (when brightness between 27 and 33%)
//! - `backlight_5` (when brightness between 34 and 40%)
//! - `backlight_6` (when brightness between 41 and 46%)
//! - `backlight_7` (when brightness between 47 and 53%)
//! - `backlight_8` (when brightness between 54 and 60%)
//! - `backlight_9` (when brightness between 61 and 67%)
//! - `backlight_10` (when brightness between 68 and 73%)
//! - `backlight_11` (when brightness between 74 and 80%)
//! - `backlight_12` (when brightness between 81 and 87%)
//! - `backlight_13` (when brightness between 88 and 93%)
//! - `backlight_full` (when brightness above 94%)

use std::cmp::max;
use std::ops::Range;
use std::path::{Path, PathBuf};

use inotify::{Inotify, WatchMask};
use tokio::fs::read_dir;

use super::prelude::*;
use crate::util::read_file;

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

/// Range of valid values for `root_scaling`
const ROOT_SCALDING_RANGE: Range<f64> = 0.1..10.;

/// Ordered list of icons used to display lighting progress
const BACKLIGHT_ICONS: &[&str] = &[
    "backlight_empty",
    "backlight_1",
    "backlight_2",
    "backlight_3",
    "backlight_4",
    "backlight_5",
    "backlight_6",
    "backlight_7",
    "backlight_8",
    "backlight_9",
    "backlight_10",
    "backlight_11",
    "backlight_12",
    "backlight_13",
    "backlight_full",
];

#[derive(Deserialize, Debug, Derivative)]
#[serde(deny_unknown_fields, default)]
#[derivative(Default)]
struct BacklightConfig {
    device: Option<String>,
    format: FormatConfig,
    #[derivative(Default(value = "5"))]
    step_width: u8,
    #[derivative(Default(value = "1"))]
    minimum: u8,
    #[derivative(Default(value = "100"))]
    maximum: u8,
    cycle: Option<Vec<u8>>,
    #[derivative(Default(value = "1.0"))]
    root_scaling: f64,
    invert_icons: bool,
}

pub async fn run(config: toml::Value, mut api: CommonApi) -> Result<()> {
    let mut events = api.get_events().await?;
    let dbus_conn = api.get_system_dbus_connection().await?;

    let config = BacklightConfig::deserialize(config).config_error()?;
    api.set_format(config.format.with_default("$brightness")?);

    let mut cycle = config
        .cycle
        .unwrap_or_else(|| vec![config.minimum, config.maximum])
        .into_iter()
        .cycle();

    let device = match &config.device {
        None => BacklightDevice::default(config.root_scaling, &dbus_conn).await?,
        Some(path) => BacklightDevice::from_device(path, config.root_scaling, &dbus_conn).await?,
    };

    // Watch for brightness changes
    let mut notify = Inotify::init().error("Failed to start inotify")?;
    let mut buffer = [0; 1024];

    notify
        .add_watch(&device.brightness_file, WatchMask::MODIFY)
        .error("Failed to watch brightness file")?;

    let mut file_changes = notify
        .event_stream(&mut buffer)
        .error("Failed to create event stream")?;

    loop {
        let brightness = device.brightness().await?;
        let mut icon_index = (usize::from(brightness) * BACKLIGHT_ICONS.len()) / 101;
        if config.invert_icons {
            icon_index = BACKLIGHT_ICONS.len() - icon_index;
        }

        api.set_icon(BACKLIGHT_ICONS[icon_index])?;
        api.set_values(map! {
            "brightness" => Value::percents(brightness as i64),
        });
        api.flush().await?;

        tokio::select! {
            _ = file_changes.next() => (),
            Some(BlockEvent::Click(event)) = events.recv() => {
                let brightness = device.brightness().await?;
                match event.button {
                    MouseButton::Left => {
                        if let Some(brightness) = cycle.next() {
                            device.set_brightness(brightness).await?;
                        }
                    }
                    MouseButton::WheelUp => {
                        device
                            .set_brightness(
                                (brightness + config.step_width)
                                    .clamp(config.minimum, config.maximum)
                            )
                            .await?;
                    }
                    MouseButton::WheelDown => {
                        device
                            .set_brightness(
                                brightness
                                    .saturating_sub(config.step_width)
                                    .clamp(config.minimum, config.maximum)
                            )
                            .await?;
                    }
                    _ => (),
                }
            }
        }
    }
}

/// Read a brightness value from the given path.
async fn read_brightness_raw(device_file: &Path) -> Result<u64> {
    read_file(device_file)
        .await
        .error("Failed to read brightness file")?
        .parse::<u64>()
        .error("Failed to read value from brightness file")
}

/// Represents a physical backlight device whose brightness level can be queried.
struct BacklightDevice<'a> {
    device_name: String,
    brightness_file: PathBuf,
    max_brightness: u64,
    root_scaling: f64,
    dbus_proxy: SessionProxy<'a>,
}

impl<'a> BacklightDevice<'a> {
    async fn new(
        device_path: PathBuf,
        root_scaling: f64,
        dbus_conn: &'a zbus::Connection,
    ) -> Result<BacklightDevice<'a>> {
        Ok(Self {
            brightness_file: device_path.join({
                if device_path.ends_with("amdgpu_bl0") {
                    FILE_BRIGHTNESS_AMD
                } else {
                    FILE_BRIGHTNESS
                }
            }),
            device_name: device_path
                .file_name()
                .map(|x| x.to_str().unwrap().into())
                .error("Malformed device path")?,
            max_brightness: read_brightness_raw(&device_path.join(FILE_MAX_BRIGHTNESS)).await?,
            root_scaling: root_scaling.clamp(ROOT_SCALDING_RANGE.start, ROOT_SCALDING_RANGE.end),
            dbus_proxy: SessionProxy::new(dbus_conn)
                .await
                .error("failed to create SessionProxy")?,
        })
    }

    /// Use the default backlit device, i.e. the first one found in the
    /// `/sys/class/backlight` directory.
    async fn default(
        root_scaling: f64,
        dbus_conn: &'a zbus::Connection,
    ) -> Result<BacklightDevice<'a>> {
        let device = read_dir(DEVICES_PATH)
            .await
            .error("Failed to read backlight device directory")?
            .next_entry()
            .await
            .error("No backlit devices found")?
            .error("Failed to read default device file")?;
        Self::new(device.path(), root_scaling, dbus_conn).await
    }

    /// Use the backlit device `device`. Returns an error if a directory for
    /// that device is not found.
    async fn from_device(
        device: &str,
        root_scaling: f64,
        dbus_conn: &'a zbus::Connection,
    ) -> Result<BacklightDevice<'a>> {
        Self::new(
            Path::new(DEVICES_PATH).join(device),
            root_scaling,
            dbus_conn,
        )
        .await
    }

    /// Query the brightness value for this backlit device, as a percent.
    async fn brightness(&self) -> Result<u8> {
        let raw = read_brightness_raw(&self.brightness_file).await?;

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
        self.dbus_proxy
            .set_brightness("backlight", &self.device_name, raw)
            .await
            .error("Failed to send D-Bus message")
    }
}

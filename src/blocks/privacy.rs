//! Privacy Monitor
//!
//! # Configuration
//!
//! Key        | Values | Default|
//! -----------|--------|--------|
//! `driver` | The configuration of a driver (see below). | **Required**
//! `format`   | Format string. | <code>\"{ $icon_audio \|}{ $icon_audio_sink \|}{ $icon_video \|}{ $icon_webcam \|}{ $icon_unknown \|}\"</code> |
//! `format_alt`   | Format string. | <code>\"{ $icon_audio $info_audio \|}{ $icon_audio_sink $info_audio_sink \|}{ $icon_video $info_video \|}{ $icon_webcam $info_webcam \|}{ $icon_unknown $info_unknown \|}\"</code> |
//!
//! # pipewire Options (requires the pipewire feature to be enabled)
//!
//! Key | Values | Required | Default
//! ----|--------|----------|--------
//! `name` | `pipewire` | Yes | None
//! `exclude_output` | An output node to ignore, example: `["HD Pro Webcam C920"]` | No | `[]`
//! `exclude_input` | An input node to ignore, example: `["openrgb"]` | No | `[]`
//! `display`   | Which node field should be used as a display name, options: `name`, `description`, `nickname` | No | `name`
//!
//! # vl4 Options
//!
//! Key | Values | Required | Default
//! ----|--------|----------|--------
//! `name` | `vl4` | Yes | None
//! `exclude_device` | A device to ignore, example: `["/dev/video5"]` | No | `[]`
//! `exclude_consumer` | Processes to ignore | No | `["pipewire", "wireplumber"]`
//!
//! # Available Format Keys
//!
//! Placeholder                                      | Value                                          | Type     | Unit
//! -------------------------------------------------|------------------------------------------------|----------|-----
//! `icon_{audio,audio_sink,video,webcam,unknown}`   | A static icon                                  | Icon     | -
//! `info_{audio,audio_sink,video,webcam,unknown}`   | The mapping of which source are being consumed | Text     | -
//!
//! You can use the suffixes noted above to get the following:
//!
//! Suffix       | Description
//! -------------|------------
//! `audio`      | Captured audio (ex. Mic)
//! `audio_sink` | Audio captured from a sink (ex. openrgb)
//! `video`      | Video capture (ex. screen capture)
//! `webcam`     | Webcam capture
//! `unknown`    | Anything else
//!
//! # Available Actions
//!
//! Action          | Description                               | Default button
//! ----------------|-------------------------------------------|---------------
//! `toggle_format` | Toggles between `format` and `format_alt` | Left
//!
//! # Example
//!
//! ```toml
//! [[block]]
//! block = "privacy"
//! [[block.driver]]
//! name = "v4l"
//! [[block.driver]]
//! name = "pipewire"
//! exclude_input = ["openrgb"]
//! display = "nickname"
//! ```
//!
//! # Icons Used
//! - `microphone`
//! - `volume`
//! - `xrandr`
//! - `webcam`
//! - `unknown`

use futures::future::{select_all, try_join_all};

use super::prelude::*;

make_log_macro!(debug, "privacy");

#[cfg(feature = "pipewire")]
mod pipewire;
mod v4l;

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub format: FormatConfig,
    #[serde(default)]
    pub format_alt: FormatConfig,
    pub driver: Vec<PrivacyDriver>,
}

#[cfg(feature = "pipewire")]
#[derive(Deserialize, Debug)]
#[serde(tag = "name", rename_all = "snake_case")]
pub enum PrivacyDriver {
    Pipewire(pipewire::Config),
    V4l(v4l::Config),
}

#[cfg(not(feature = "pipewire"))]
#[derive(Deserialize, Debug)]
#[serde(tag = "name", rename_all = "snake_case")]
pub enum PrivacyDriver {
    V4l(v4l::Config),
}

#[derive(Debug, Clone, Eq, Hash, PartialEq)]
enum Type {
    Audio,
    AudioSink,
    Video,
    Webcam,
    Unknown,
}

// {type: {source: {destination: count}}
type PrivacyInfo = HashMap<Type, PrivacyInfoInner>;

type PrivacyInfoInnerType = HashMap<String, HashMap<String, usize>>;
#[derive(Default, Debug)]
struct PrivacyInfoInner(PrivacyInfoInnerType);

impl std::ops::Deref for PrivacyInfoInner {
    type Target = PrivacyInfoInnerType;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for PrivacyInfoInner {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl std::fmt::Display for PrivacyInfoInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{{ {} }}",
            itertools::join(
                self.iter().map(|(source, destinations)| {
                    format!(
                        "{} => [ {} ]",
                        source,
                        itertools::join(
                            destinations
                                .iter()
                                .map(|(destination, count)| if count == &1 {
                                    destination.into()
                                } else {
                                    format!("{} (x{})", destination, count)
                                }),
                            ", "
                        )
                    )
                }),
                ", ",
            )
        )
    }
}

#[async_trait]
trait PrivacyMonitor {
    async fn get_info(&mut self) -> Result<PrivacyInfo>;
    async fn wait_for_change(&mut self) -> Result<()>;
}

pub async fn run(config: &Config, api: &CommonApi) -> Result<()> {
    let mut actions = api.get_actions()?;
    api.set_default_actions(&[(MouseButton::Left, None, "toggle_format")])?;

    let mut format = config.format.with_default(
        "{ $icon_audio |}{ $icon_audio_sink |}{ $icon_video |}{ $icon_webcam |}{ $icon_unknown |}",
    )?;
    let mut format_alt = config.format_alt.with_default("{ $icon_audio $info_audio |}{ $icon_audio_sink $info_audio_sink |}{ $icon_video $info_video |}{ $icon_webcam $info_webcam |}{ $icon_unknown $info_unknown |}")?;

    let mut drivers: Vec<Box<dyn PrivacyMonitor + Send + Sync>> = Vec::new();

    for driver in &config.driver {
        drivers.push(match driver {
            #[cfg(feature = "pipewire")]
            PrivacyDriver::Pipewire(driver_config) => {
                Box::new(pipewire::Monitor::new(driver_config).await?)
            }
            PrivacyDriver::V4l(driver_config) => {
                Box::new(v4l::Monitor::new(driver_config, api.error_interval).await?)
            }
        });
    }

    loop {
        let mut widget = Widget::new().with_format(format.clone());

        let mut info = PrivacyInfo::default();
        //Merge driver info
        for driver_info in try_join_all(drivers.iter_mut().map(|driver| driver.get_info())).await? {
            for (type_, mapping) in driver_info {
                let existing_mapping = info.entry(type_).or_default();
                for (source, dest) in mapping.0 {
                    existing_mapping.entry(source).or_default().extend(dest);
                }
            }
        }
        if !info.is_empty() {
            widget.state = State::Warning;
        }

        let mut values = Values::new();

        if let Some(info_by_type) = info.get(&Type::Audio) {
            map! { @extend values
                "icon_audio" => Value::icon("microphone"),
                "info_audio" => Value::text(format!("{}", info_by_type))
            }
        }
        if let Some(info_by_type) = info.get(&Type::AudioSink) {
            map! { @extend values
                "icon_audio_sink" => Value::icon("volume"),
                "info_audio_sink" => Value::text(format!("{}", info_by_type))
            }
        }
        if let Some(info_by_type) = info.get(&Type::Video) {
            map! { @extend values
                "icon_video" => Value::icon("xrandr"),
                "info_video" => Value::text(format!("{}", info_by_type))
            }
        }
        if let Some(info_by_type) = info.get(&Type::Webcam) {
            map! { @extend values
                "icon_webcam" => Value::icon("webcam"),
                "info_webcam" => Value::text(format!("{}", info_by_type))
            }
        }
        if let Some(info_by_type) = info.get(&Type::Unknown) {
            map! { @extend values
                "icon_unknown" => Value::icon("unknown"),
                "info_unknown" => Value::text(format!("{}", info_by_type))
            }
        }

        widget.set_values(values);

        api.set_widget(widget)?;

        select! {
            _ = api.wait_for_update_request() => (),
            _ = select_all(drivers.iter_mut().map(|driver| driver.wait_for_change())) =>(),
            Some(action) = actions.recv() => match action.as_ref() {
                "toggle_format" => {
                    std::mem::swap(&mut format_alt, &mut format);
                }
                _ => (),
            }
        }
    }
}

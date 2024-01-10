//! Privacy Monitor
//!
//! # Configuration
//!
//! Key        | Values | Default|
//! -----------|--------|--------|
//! `driver` | The configuration of a driver (see below). | **Required**
//! `format`   | Format string. | <code>"{ $icon_audio \|}{ $icon_audio_sink \|}{ $icon_video \|}{ $icon_webcam \|}{ $icon_unknown \|}"</code> |
//! `format_alt`   | Format string. | <code>"{ $icon_audio $info_audio \|}{ $icon_audio_sink $info_audio_sink \|}{ $icon_video $info_video \|}{ $icon_webcam $info_webcam \|}{ $icon_unknown $info_unknown \|}"</code> |
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
//! [block.driver]
//! name = "v4l"
//! ```
//!
//! # Icons Used
//! - `microphone`
//! - `volume`
//! - `xrandr`
//! - `webcam`
//! - `unknown`

use std::collections::HashSet;

use super::prelude::*;

make_log_macro!(debug, "privacy");

mod v4l;

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub format: FormatConfig,
    #[serde(default)]
    pub format_alt: FormatConfig,
    pub driver: PrivacyDriver,
}

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

// {type: {name: {reader}}
type PrivacyInfo = HashMap<Type, HashMap<String, HashSet<String>>>;

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

    let mut device: Box<dyn PrivacyMonitor + Send + Sync> = match &config.driver {
        PrivacyDriver::V4l(driver_config) => {
            Box::new(v4l::Monitor::new(driver_config, api.error_interval).await?)
        }
    };

    loop {
        let mut widget = Widget::new().with_format(format.clone());

        let info = device.get_info().await?;
        if !info.is_empty() {
            widget.state = State::Warning;
        }

        let mut values = Values::new();

        if let Some(info_by_type) = info.get(&Type::Audio) {
            values.extend(map! {
                "icon_audio" => Value::icon("microphone"),
                "info_audio" => Value::text(format!("{:?}", info_by_type))
            });
        }
        if let Some(info_by_type) = info.get(&Type::AudioSink) {
            values.extend(map! {
                "icon_audio_sink" => Value::icon("volume"),
                "info_audio_sink" => Value::text(format!("{:?}", info_by_type))
            });
        }
        if let Some(info_by_type) = info.get(&Type::Video) {
            values.extend(map! {
                "icon_video" => Value::icon("xrandr"),
                "info_video" => Value::text(format!("{:?}", info_by_type))
            });
        }
        if let Some(info_by_type) = info.get(&Type::Webcam) {
            values.extend(map! {
                "icon_webcam" => Value::icon("webcam"),
                "info_webcam" => Value::text(format!("{:?}", info_by_type))
            });
        }
        if let Some(info_by_type) = info.get(&Type::Unknown) {
            values.extend(map! {
                "icon_unknown" => Value::icon("unknown"),
                "info_unknown" => Value::text(format!("{:?}", info_by_type))
            });
        }

        widget.set_values(values);

        api.set_widget(widget)?;

        select! {
            _ = api.wait_for_update_request() => (),
            _ = device.wait_for_change() =>(),
            Some(action) = actions.recv() => match action.as_ref() {
                "toggle_format" => {
                    std::mem::swap(&mut format_alt, &mut format);
                }
                _ => (),
            }
        }
    }
}

//! Privacy Monitor
//!
//! # Configuration
//!
//! Key        | Values | Default|
//! -----------|--------|--------|
//! `driver` | The configuration of a driver (see below). | **Required**
//! `format`   | [MultiFormat][MaybeMultiFormatConfig] string. | <code>[\"{ $icon_audio \|}{ $icon_audio_sink \|}{ $icon_video \|}{ $icon_webcam \|}{ $icon_unknown \|}\", \"{ $icon_audio $info_audio \|}{ $icon_audio_sink $info_audio_sink \|}{ $icon_video $info_video \|}{ $icon_webcam $info_webcam \|}{ $icon_unknown $info_unknown \|}\"]</code> |
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
//! `toggle_format` **DEPRECATED** | Toggles between `format` and `format_alt` | -
//! `next_format`  | Switches to the next format in the list     | Left
//! `prev_format`  | Switches to the previous format in the list | Right
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
pub struct Config {
    #[serde(flatten)]
    pub formats: MaybeMultiFormatConfig,
    pub driver: Vec<PrivacyDriver>,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "name", rename_all = "snake_case")]
pub enum PrivacyDriver {
    #[cfg(feature = "pipewire")]
    Pipewire(pipewire::Config),
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
                                    format!("{destination} (x{count})")
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

/// The icon each icon-valued placeholder carries, per capture type.
const TYPE_ICONS: [(Type, &str, &str); 5] = [
    (Type::Audio, "icon_audio", "microphone"),
    (Type::AudioSink, "icon_audio_sink", "volume"),
    (Type::Video, "icon_video", "xrandr"),
    (Type::Webcam, "icon_webcam", "webcam"),
    (Type::Unknown, "icon_unknown", "unknown"),
];

impl PrivacyDriver {
    /// The capture types this driver's monitor can report.
    fn capture_types(&self) -> &'static [Type] {
        match self {
            #[cfg(feature = "pipewire")]
            PrivacyDriver::Pipewire(_) => &[
                Type::Audio,
                Type::AudioSink,
                Type::Video,
                Type::Webcam,
                Type::Unknown,
            ],
            PrivacyDriver::V4l(_) => &[Type::Webcam],
        }
    }
}

pub(crate) fn prepare(config: &Config) -> Result<Arc<BlockPlan>> {
    let format = config.format.with_default(
        "{ $icon_audio |}{ $icon_audio_sink |}{ $icon_video |}{ $icon_webcam |}{ $icon_unknown |}",
    )?;
    let format_alt = config.format_alt.with_default("{ $icon_audio $info_audio |}{ $icon_audio_sink $info_audio_sink |}{ $icon_video $info_video |}{ $icon_webcam $info_webcam |}{ $icon_unknown $info_unknown |}")?;

    // Only capture types some configured driver can actually report are
    // declared: a V4L-only setup can never set the audio or video icons.
    let with_icons = |mut output: OutputPlan| {
        for (ty, placeholder, icon) in &TYPE_ICONS {
            if config
                .driver
                .iter()
                .any(|driver| driver.capture_types().contains(ty))
            {
                output = output.icon(placeholder, IconChoices::one(*icon));
            }
        }
        output
    };
    BlockPlan::new(vec![
        with_icons(OutputPlan::new("main", format)),
        with_icons(OutputPlan::new("alt", format_alt)),
    ])
}

pub async fn run(config: &Config, api: &CommonApi, plan: &Arc<BlockPlan>) -> Result<()> {
    let mut actions = api.get_actions()?;
    api.set_default_actions(&[
        (MouseButton::Left, None, "next_format"),
        (MouseButton::Right, None, "prev_format"),
    ])?;

    let output_main = plan.output("main")?;
    let output_alt = plan.output("alt")?;
    let mut alt_shown = false;

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
        let output = if alt_shown { &output_alt } else { &output_main };
        let mut widget = output.new_widget();

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
                "icon_audio" => output.named_icon_value("icon_audio", "microphone")?,
                "info_audio" => Value::text(info_by_type.to_string())
            }
        }
        if let Some(info_by_type) = info.get(&Type::AudioSink) {
            map! { @extend values
                "icon_audio_sink" => output.named_icon_value("icon_audio_sink", "volume")?,
                "info_audio_sink" => Value::text(info_by_type.to_string())
            }
        }
        if let Some(info_by_type) = info.get(&Type::Video) {
            map! { @extend values
                "icon_video" => output.named_icon_value("icon_video", "xrandr")?,
                "info_video" => Value::text(info_by_type.to_string())
            }
        }
        if let Some(info_by_type) = info.get(&Type::Webcam) {
            map! { @extend values
                "icon_webcam" => output.named_icon_value("icon_webcam", "webcam")?,
                "info_webcam" => Value::text(info_by_type.to_string())
            }
        }
        if let Some(info_by_type) = info.get(&Type::Unknown) {
            map! { @extend values
                "icon_unknown" => output.named_icon_value("icon_unknown", "unknown")?,
                "info_unknown" => Value::text(info_by_type.to_string())
            }
        }

        widget.set_values(values);

        api.set_widget(widget)?;

        select! {
            _ = api.wait_for_update_request() => (),
            _ = select_all(drivers.iter_mut().map(|driver| driver.wait_for_change())) =>(),
            Some(action) = actions.recv() => match action.as_ref() {
                "toggle_format" => {
                    alt_shown = !alt_shown;
                }
                _ => (),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Config {
        Config {
            format: Default::default(),
            format_alt: Default::default(),
            driver: Vec::new(),
        }
    }

    fn config_with_drivers(toml_drivers: &str) -> Config {
        toml::from_str(toml_drivers).unwrap()
    }

    #[test]
    fn plan_declares_driver_reachable_icons_on_both_outputs() {
        // A V4L-only configuration can only ever report webcam capture.
        let config = config_with_drivers("[[driver]]\nname = \"v4l\"");
        let plan = prepare(&config).unwrap();
        let ids: Vec<_> = plan.outputs().map(|o| o.id()).collect();
        assert_eq!(ids, ["main", "alt"]);
        for id in ids {
            let output = plan.output(id).unwrap();
            assert_eq!(output.single_icon("icon_webcam").unwrap(), "webcam");
            assert_eq!(
                output.output().icon_placeholders().count(),
                1,
                "{id} must declare only V4L-reachable icons"
            );
        }
    }

    #[cfg(feature = "pipewire")]
    #[test]
    fn pipewire_declares_every_type_icon() {
        let config = config_with_drivers("[[driver]]\nname = \"pipewire\"");
        let plan = prepare(&config).unwrap();
        for id in ["main", "alt"] {
            let output = plan.output(id).unwrap();
            for (_, placeholder, icon) in &TYPE_ICONS {
                assert_eq!(
                    output.single_icon(placeholder).unwrap(),
                    *icon,
                    "{id} must declare {icon} for ${placeholder}"
                );
            }
        }
    }


    #[test]
    fn every_capture_type_has_a_declared_icon() {
        // Drift test: the exhaustive match stops compiling when a `Type`
        // variant is added, forcing TYPE_ICONS (and the value mapping in
        // `run()`) to be extended in lockstep.
        let types = [
            Type::Audio,
            Type::AudioSink,
            Type::Video,
            Type::Webcam,
            Type::Unknown,
        ];
        assert_eq!(types.len(), TYPE_ICONS.len());
        for type_ in types {
            let (placeholder, icon) = match type_ {
                Type::Audio => ("icon_audio", "microphone"),
                Type::AudioSink => ("icon_audio_sink", "volume"),
                Type::Video => ("icon_video", "xrandr"),
                Type::Webcam => ("icon_webcam", "webcam"),
                Type::Unknown => ("icon_unknown", "unknown"),
            };
            assert!(
                TYPE_ICONS
                    .iter()
                    .any(|(t, p, i)| *t == type_ && *p == placeholder && *i == icon)
            );
        }
    }

    #[test]
    fn alt_output_uses_the_alternate_format() {
        let plan = prepare(&config()).unwrap();
        assert!(
            !plan
                .output("main")
                .unwrap()
                .format()
                .contains_key("info_audio")
        );
        assert!(
            plan.output("alt")
                .unwrap()
                .format()
                .contains_key("info_audio")
        );
    }
}

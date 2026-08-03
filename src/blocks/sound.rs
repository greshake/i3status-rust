//! Volume level
//!
//! This block displays the volume level (according to PulseAudio or ALSA). Right click to toggle mute, scroll to adjust volume.
//!
//! Requires a PulseAudio installation or `alsa-utils` for ALSA.
//!
//! Note that if you are using PulseAudio commands (such as `pactl`) to control your volume, you should select the `"pulseaudio"` (or `"auto"`) driver to see volume changes that exceed 100%.
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `driver` | `"auto"`, `pipewire`, `"pulseaudio"`, `"alsa"`. | `"auto"` (Pipewire with Pulseaudio fallback with ALSA fallback)
//! `format` | A [MultiFormat][MaybeMultiFormatConfig] string to customise the output of this block. See below for available placeholders. | <code>[\" $icon {$volume.eng(w:2) \|}\"]</code>
//! `name` | PulseAudio device name, or the ALSA control name as found in the output of `amixer -D yourdevice scontrols`. | PulseAudio: `@DEFAULT_SINK@` / ALSA: `Master`
//! `device` | ALSA device name, usually in the form "hw:X" or "hw:X,Y" where `X` is the card number and `Y` is the device number as found in the output of `aplay -l`. | `default`
//! `device_kind` | PulseAudio device kind: `source` or `sink`. | `"sink"`
//! `natural_mapping` | When using the ALSA driver, display the "mapped volume" as given by `alsamixer`/`amixer -M`, which represents the volume level more naturally with respect for the human ear. | `false`
//! `step_width` | The percent volume level is increased/decreased for the selected audio device when scrolling. Capped automatically at 50. | `5`
//! `max_vol` | Max volume in percent that can be set via scrolling. Note it can still be set above this value if changed by another application. | `None`
//! `show_volume_when_muted` | Show the volume even if it is currently muted. | `false`
//! `headphones_indicator` | Change icon when headphones are plugged in (pulseaudio only) | `false`
//! `mappings` | Map `output_name` to a custom name. | `None`
//! `mappings_use_regex` | Let `mappings` match using regex instead of string equality. The replacement will be regex aware and can contain capture groups. | `true`
//! `active_port_mappings` | Map `active_port` to a custom name. The replacement will be regex aware and can contain capture groups. | `None`
//!
//! Placeholder          | Value                             | Type   | Unit
//! ---------------------|-----------------------------------|--------|---------------
//! `icon`               | Icon based on volume              | Icon   | -
//! `volume`             | Current volume. Missing if muted. | Number | %
//! `output_name`        | PulseAudio or ALSA device name    | Text   | -
//! `output_description` | PulseAudio device description, will fallback to `output_name` if no description is available and will be overwritten by mappings (mappings will still use `output_name`) | Text | -
//! `active_port`        | Active port (same as information in Ports section of `pactl list cards`). Will be absent if not supported by `driver` or if mapped to `""` in `active_port_mappings`. | Text | -
//!
//! Action          | Default button
//! ----------------|---------------
//! `toggle_mute`   | Right
//! `volume_down`   | Wheel Down
//! `volume_up`     | Wheel Up
//! `toggle_format` **DEPRECATED** | Toggles between `format` and `format_alt` | -
//! `next_format`  | Switches to the next format in the list     | Left
//! `prev_format`  | Switches to the previous format in the list | -
//!
//! # Examples
//!
//! Change the default scrolling step width to 3 percent:
//!
//! ```toml
//! [[block]]
//! block = "sound"
//! step_width = 3
//! ```
//!
//! Change the output name shown:
//!
//! ```toml
//! [[block]]
//! block = "sound"
//! format = " $icon $output_name{ $volume|} "
//! [block.mappings]
//! "alsa_output.usb-Harman_Multimedia_JBL_Pebbles_1.0.0-00.analog-stereo" = "Speakers"
//! "alsa_output.pci-0000_00_1b.0.analog-stereo" = "Headset"
//! ```
//!
//! Since the default value for the `device_kind` key is `sink`,
//! to display ***microphone*** block you have to use the `source` value:
//!
//! ```toml
//! [[block]]
//! block = "sound"
//! driver = "pulseaudio"
//! device_kind = "source"
//! ```
//!
//! Display warning in block if microphone if using the wrong port:
//!
//! ```toml
//! [[block]]
//! block = "sound"
//! driver = "pulseaudio"
//! device_kind = "source"
//! format = " $icon { $volume|} {$active_port |}"
//! [block.active_port_mappings]
//! "analog-input-rear-mic" = "" # Mapping to an empty string makes `$active_port` absent
//! "analog-input-front-mic" = "ERR!"
//! ```
//!
//! #  Icons Used
//!
//! - `microphone_muted` (`$icon`, as a progression)
//! - `microphone` (`$icon`, as a progression)
//! - `volume_muted` (`$icon`, as a progression)
//! - `volume` (`$icon`, as a progression)
//! - `headphones` (`$icon`)

make_log_macro!(debug, "sound");

mod alsa;
#[cfg(feature = "pipewire")]
pub mod pipewire;
#[cfg(feature = "pulseaudio")]
mod pulseaudio;

use super::prelude::*;
use crate::wrappers::SerdeRegex;
use indexmap::IndexMap;
use regex::Regex;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(default)]
pub struct Config {
    pub driver: SoundDriver,
    pub name: Option<String>,
    pub device: Option<String>,
    pub device_kind: DeviceKind,
    pub natural_mapping: bool,
    #[default(5)]
    pub step_width: u32,
    #[serde(flatten)]
    pub formats: MaybeMultiFormatConfig,
    pub headphones_indicator: bool,
    pub show_volume_when_muted: bool,
    pub mappings: Option<IndexMap<String, String>>,
    #[default(true)]
    pub mappings_use_regex: bool,
    pub max_vol: Option<u32>,
    pub active_port_mappings: IndexMap<SerdeRegex, String>,
}

enum Mappings<'a> {
    Exact(&'a IndexMap<String, String>),
    Regex(Vec<(Regex, &'a str)>),
}

/// Whether the device should be represented by the `headphones` icon.
fn is_headphones(device: &dyn SoundDevice) -> bool {
    let form_factor = device.form_factor();
    let active_port = device.active_port();
    debug!("form_factor = {form_factor:?} active_port = {active_port:?}");
    match form_factor {
        // form_factor's possible values are listed at:
        // https://docs.rs/libpulse-binding/2.25.0/libpulse_binding/proplist/properties/constant.DEVICE_FORM_FACTOR.html
        Some("headset") | Some("headphone") | Some("hands-free") | Some("portable") => true,
        // Per discussion at
        // https://github.com/greshake/i3status-rust/pull/1363#issuecomment-1046095869,
        // fall back to checking active_port if form_factor is absent, unknown, or doesn't match
        // known headphone values (common on PipeWire/WirePlumber systems).
        _ => active_port
            .as_ref()
            .is_some_and(|p| p.to_lowercase().contains("headphone")),
    }
}

/// The icon name for the current device state. `headphones` can only be true
/// when `headphones_indicator` is set and the device is a sink.
fn icon_name(device_kind: DeviceKind, headphones: bool, muted: bool) -> &'static str {
    if headphones {
        return "headphones";
    }
    if muted {
        match device_kind {
            DeviceKind::Source => "microphone_muted",
            DeviceKind::Sink => "volume_muted",
        }
    } else {
        match device_kind {
            DeviceKind::Source => "microphone",
            DeviceKind::Sink => "volume",
        }
    }
}

/// Every name [`icon_name`] can return for this configuration. The runtime
/// state (muted, headphones plugged in) is externally selected but finite,
/// so the block plan declares the full set.
fn declared_icon_names(device_kind: DeviceKind, headphones_indicator: bool) -> Vec<&'static str> {
    let mut names = match device_kind {
        DeviceKind::Sink => vec!["volume", "volume_muted"],
        DeviceKind::Source => vec!["microphone", "microphone_muted"],
    };
    if headphones_indicator && device_kind == DeviceKind::Sink {
        names.push("headphones");
    }
    names
}

pub(crate) fn prepare(config: &Config) -> Result<Arc<BlockPlan>> {
    let icons = || {
        IconChoices::fixed(declared_icon_names(
            config.device_kind,
            config.headphones_indicator,
        ))
    };
    // `volume` is removed when muted (unless `show_volume_when_muted`) and
    // `active_port` can be absent or mapped away, so neither is guaranteed.
    let with_guarantees = |output: OutputPlan| output.icon("icon", icons());
    let mut outputs = vec![with_guarantees(OutputPlan::new(
        "main",
        config.format.with_default(" $icon {$volume.eng(w:2)|} ")?,
    ))];
    if let Some(format_alt) = &config.format_alt {
        outputs.push(with_guarantees(OutputPlan::new(
            "alt",
            format_alt.with_default("")?,
        )));
    }
    BlockPlan::new(outputs)
}

pub async fn run(config: &Config, api: &CommonApi, plan: &Arc<BlockPlan>) -> Result<()> {
    let mut actions = api.get_actions()?;
    api.set_default_actions(&[
        (MouseButton::Left, None, "next_format"),
        (MouseButton::Right, None, "toggle_mute"),
        (MouseButton::WheelUp, None, "volume_up"),
        (MouseButton::WheelDown, None, "volume_down"),
    ])?;

    let output_main = plan.output("main")?;
    let output_alt = match &config.format_alt {
        Some(_) => Some(plan.output("alt")?),
        None => None,
    };
    let mut alt_shown = false;

    let device_kind = config.device_kind;
    let step_width = config.step_width.clamp(0, 50) as i32;

    type DeviceType = Box<dyn SoundDevice>;
    let mut device: DeviceType = match config.driver {
        SoundDriver::Alsa => Box::new(alsa::Device::new(
            config.name.clone().unwrap_or_else(|| "Master".into()),
            config.device.clone().unwrap_or_else(|| "default".into()),
            config.natural_mapping,
        )?),
        #[cfg(feature = "pipewire")]
        SoundDriver::Pipewire => {
            Box::new(pipewire::Device::new(config.device_kind, config.name.clone()).await?)
        }
        #[cfg(feature = "pulseaudio")]
        SoundDriver::PulseAudio => Box::new(pulseaudio::Device::new(
            config.device_kind,
            config.name.clone(),
        )?),
        SoundDriver::Auto => 'blk: {
            #[cfg(feature = "pulseaudio")]
            if let Ok(pulse) = pulseaudio::Device::new(config.device_kind, config.name.clone()) {
                break 'blk Box::new(pulse);
            }
            #[cfg(feature = "pipewire")]
            if let Ok(pipewire) =
                pipewire::Device::new(config.device_kind, config.name.clone()).await
            {
                break 'blk Box::new(pipewire);
            }
            Box::new(alsa::Device::new(
                config.name.clone().unwrap_or_else(|| "Master".into()),
                config.device.clone().unwrap_or_else(|| "default".into()),
                config.natural_mapping,
            )?)
        }
    };

    let mappings = match &config.mappings {
        Some(m) => {
            if config.mappings_use_regex {
                Some(Mappings::Regex(
                    m.iter()
                        .map(|(key, val)| {
                            Ok((
                                Regex::new(key)
                                    .error("Failed to parse `{key}` in mappings as regex")?,
                                val.as_str(),
                            ))
                        })
                        .collect::<Result<_>>()?,
                ))
            } else {
                Some(Mappings::Exact(m))
            }
        }
        None => None,
    };

    loop {
        device.get_info().await?;
        let volume = device.volume();
        let muted = device.muted();
        let mut output_name = device.output_name();
        let mut active_port = device.active_port();
        match &mappings {
            Some(Mappings::Regex(m)) => {
                if let Some((regex, mapped)) =
                    m.iter().find(|(regex, _)| regex.is_match(&output_name))
                {
                    output_name = regex.replace(&output_name, *mapped).into_owned();
                }
            }
            Some(Mappings::Exact(m)) => {
                if let Some(mapped) = m.get(&output_name) {
                    output_name.clone_from(mapped);
                }
            }
            None => (),
        }
        if let Some(ap) = &active_port
            && let Some((regex, mapped)) = config
                .active_port_mappings
                .iter()
                .find(|(regex, _)| regex.0.is_match(ap))
        {
            let mapped = regex.0.replace(ap, mapped);
            if mapped.is_empty() {
                active_port = None;
            } else {
                active_port = Some(mapped.into_owned());
            }
        }

        let output_description = device
            .output_description()
            .unwrap_or_else(|| output_name.clone());

        let headphones = config.headphones_indicator
            && device_kind == DeviceKind::Sink
            && is_headphones(&*device);

        let output = match (&output_alt, alt_shown) {
            (Some(alt), true) => alt,
            _ => &output_main,
        };
        let mut values = map! {
            "icon" => output.icon_progression(
                "icon",
                icon_name(device_kind, headphones, muted),
                volume as f64 / 100.0,
            )?,
            "volume" => Value::percents(volume),
            "output_name" => Value::text(output_name),
            "output_description" => Value::text(output_description),
            [if let Some(ap) = active_port] "active_port" => Value::text(ap),
        };
        let mut widget = output.new_widget();

        if muted {
            widget.state = State::Warning;
            if !config.show_volume_when_muted {
                values.remove("volume");
            }
        }

        widget.set_values(values);
        api.set_widget(widget)?;

        loop {
            select! {
                val = device.wait_for_update() => {
                    val?;
                    break;
                }
                _ = api.wait_for_update_request() => break,
                Some(action) = actions.recv() => match action.as_ref() {
                    "toggle_format" if output_alt.is_some() => {
                        alt_shown = !alt_shown;
                        break;
                    }
                    "toggle_mute" => {
                        device.toggle().await?;
                    }
                    "volume_up" => {
                        device.set_volume(step_width, config.max_vol).await?;
                    }
                    "volume_down" => {
                        device.set_volume(-step_width, config.max_vol).await?;
                    }
                    _ => (),
                }
            }
        }
    }
}

#[derive(Deserialize, Debug, SmartDefault, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum SoundDriver {
    #[default]
    Auto,
    Alsa,
    #[cfg(feature = "pipewire")]
    Pipewire,
    #[cfg(feature = "pulseaudio")]
    PulseAudio,
}

#[derive(Deserialize, Debug, SmartDefault, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum DeviceKind {
    #[default]
    Sink,
    Source,
}

#[async_trait::async_trait]
trait SoundDevice {
    fn volume(&self) -> u32;
    fn muted(&self) -> bool;
    fn output_name(&self) -> String;
    fn output_description(&self) -> Option<String>;
    fn active_port(&self) -> Option<String>;
    fn form_factor(&self) -> Option<&str>;

    async fn get_info(&mut self) -> Result<()>;
    async fn set_volume(&mut self, step: i32, max_vol: Option<u32>) -> Result<()>;
    async fn toggle(&mut self) -> Result<()>;
    async fn wait_for_update(&mut self) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_declares_device_kind_specific_icons() {
        // Default: sink without headphones indicator.
        let plan = prepare(&Config::default()).unwrap();
        let ids: Vec<_> = plan.outputs().map(|o| o.id()).collect();
        assert_eq!(ids, ["main"]);
        let choices = plan
            .output("main")
            .unwrap()
            .output()
            .choices_for("icon")
            .unwrap()
            .clone();
        assert!(choices.permits("volume"));
        assert!(choices.permits("volume_muted"));
        assert!(!choices.permits("headphones"));
        assert!(!choices.permits("microphone"));
        assert!(!choices.permits("microphone_muted"));

        // Source devices use the microphone icons.
        let config = Config {
            device_kind: DeviceKind::Source,
            ..Config::default()
        };
        let plan = prepare(&config).unwrap();
        let choices = plan
            .output("main")
            .unwrap()
            .output()
            .choices_for("icon")
            .unwrap()
            .clone();
        assert!(choices.permits("microphone"));
        assert!(choices.permits("microphone_muted"));
        assert!(!choices.permits("volume"));
        assert!(!choices.permits("headphones"));
    }

    #[test]
    fn headphones_icon_declared_only_for_sinks_with_indicator() {
        let config = Config {
            headphones_indicator: true,
            ..Config::default()
        };
        let plan = prepare(&config).unwrap();
        assert!(
            plan.output("main")
                .unwrap()
                .output()
                .choices_for("icon")
                .unwrap()
                .permits("headphones")
        );

        // The indicator only applies to sinks.
        let config = Config {
            headphones_indicator: true,
            device_kind: DeviceKind::Source,
            ..Config::default()
        };
        let plan = prepare(&config).unwrap();
        assert!(
            !plan
                .output("main")
                .unwrap()
                .output()
                .choices_for("icon")
                .unwrap()
                .permits("headphones")
        );
    }

    #[test]
    fn alt_output_exists_only_when_configured() {
        let plan = prepare(&Config::default()).unwrap();
        assert!(plan.output("alt").is_err());

        let config = Config {
            format_alt: Some(" $icon $output_name ".parse().unwrap()),
            ..Config::default()
        };
        let plan = prepare(&config).unwrap();
        let alt = plan.output("alt").unwrap();
        assert!(alt.format().contains_key("output_name"));
        assert!(alt.output().choices_for("icon").unwrap().permits("volume"));
    }

    #[test]
    fn chooser_only_produces_declared_names() {
        for device_kind in [DeviceKind::Sink, DeviceKind::Source] {
            for headphones_indicator in [false, true] {
                let declared = declared_icon_names(device_kind, headphones_indicator);
                // Headphones can only be detected for sinks with the
                // indicator enabled; mirror that reachability here.
                let headphone_states: &[bool] =
                    if headphones_indicator && device_kind == DeviceKind::Sink {
                        &[false, true]
                    } else {
                        &[false]
                    };
                for &headphones in headphone_states {
                    for muted in [false, true] {
                        let name = icon_name(device_kind, headphones, muted);
                        assert!(
                            declared.contains(&name),
                            "icon '{name}' not declared for {device_kind:?} \
                             (headphones_indicator: {headphones_indicator})"
                        );
                    }
                }
            }
        }
    }
}

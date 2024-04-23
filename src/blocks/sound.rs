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
//! `driver` | `"auto"`, `"pulseaudio"`, `"alsa"`. | `"auto"` (Pulseaudio with ALSA fallback)
//! `format` | A string to customise the output of this block. See below for available placeholders. | <code>\" $icon {$volume.eng(w:2) \|}\"</code>
//! `format_alt` | If set, block will switch between `format` and `format_alt` on every click. | `None`
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
//! `toggle_format` | Left
//! `toggle_mute`   | Right
//! `volume_down`   | Wheel Down
//! `volume_up`     | Wheel Up
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
//! - `microphone_muted` (as a progression)
//! - `microphone` (as a progression)
//! - `volume_muted` (as a progression)
//! - `volume` (as a progression)
//! - `headphones`

mod alsa;
#[cfg(feature = "pulseaudio")]
mod pulseaudio;

use super::prelude::*;
use crate::wrappers::SerdeRegex;
use indexmap::IndexMap;
use regex::Regex;

make_log_macro!(debug, "sound");

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub driver: SoundDriver,
    pub name: Option<String>,
    pub device: Option<String>,
    pub device_kind: DeviceKind,
    pub natural_mapping: bool,
    #[default(5)]
    pub step_width: u32,
    pub format: FormatConfig,
    pub format_alt: Option<FormatConfig>,
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

pub async fn run(config: &Config, api: &CommonApi) -> Result<()> {
    let mut actions = api.get_actions()?;
    api.set_default_actions(&[
        (MouseButton::Left, None, "toggle_format"),
        (MouseButton::Right, None, "toggle_mute"),
        (MouseButton::WheelUp, None, "volume_up"),
        (MouseButton::WheelDown, None, "volume_down"),
    ])?;

    let mut format = config.format.with_default(" $icon {$volume.eng(w:2)|} ")?;
    let mut format_alt = match &config.format_alt {
        Some(f) => Some(f.with_default("")?),
        None => None,
    };

    let device_kind = config.device_kind;
    let step_width = config.step_width.clamp(0, 50) as i32;

    let icon = |muted: bool, device: &dyn SoundDevice| -> &'static str {
        if config.headphones_indicator && device_kind == DeviceKind::Sink {
            let form_factor = device.form_factor();
            let active_port = device.active_port();
            debug!("form_factor = {form_factor:?} active_port = {active_port:?}");
            let headphones = match form_factor {
                // form_factor's possible values are listed at:
                // https://docs.rs/libpulse-binding/2.25.0/libpulse_binding/proplist/properties/constant.DEVICE_FORM_FACTOR.html
                Some("headset") | Some("headphone") | Some("hands-free") | Some("portable") => true,
                // Per discussion at
                // https://github.com/greshake/i3status-rust/pull/1363#issuecomment-1046095869,
                // some sinks may not have the form_factor property, so we should fall back to the
                // active_port if that property is not present.
                None => active_port.is_some_and(|p| p.to_lowercase().contains("headphones")),
                // form_factor is present and is some non-headphone value
                _ => false,
            };
            if headphones {
                return "headphones";
            }
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
    };

    type DeviceType = Box<dyn SoundDevice>;
    let mut device: DeviceType = match config.driver {
        SoundDriver::Alsa => Box::new(alsa::Device::new(
            config.name.clone().unwrap_or_else(|| "Master".into()),
            config.device.clone().unwrap_or_else(|| "default".into()),
            config.natural_mapping,
        )?),
        #[cfg(feature = "pulseaudio")]
        SoundDriver::PulseAudio => Box::new(pulseaudio::Device::new(
            config.device_kind,
            config.name.clone(),
        )?),
        #[cfg(feature = "pulseaudio")]
        SoundDriver::Auto => {
            if let Ok(pulse) = pulseaudio::Device::new(config.device_kind, config.name.clone()) {
                Box::new(pulse)
            } else {
                Box::new(alsa::Device::new(
                    config.name.clone().unwrap_or_else(|| "Master".into()),
                    config.device.clone().unwrap_or_else(|| "default".into()),
                    config.natural_mapping,
                )?)
            }
        }
        #[cfg(not(feature = "pulseaudio"))]
        SoundDriver::Auto => Box::new(alsa::Device::new(
            config.name.clone().unwrap_or_else(|| "Master".into()),
            config.device.clone().unwrap_or_else(|| "default".into()),
            config.natural_mapping,
        )?),
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
        if let Some(ap) = &active_port {
            if let Some((regex, mapped)) = config
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
        }

        let output_description = device
            .output_description()
            .unwrap_or_else(|| output_name.clone());

        let mut values = map! {
            "icon" => Value::icon_progression(icon(muted, &*device), volume as f64 / 100.0),
            "volume" => Value::percents(volume),
            "output_name" => Value::text(output_name),
            "output_description" => Value::text(output_description),
            [if let Some(ap) = active_port] "active_port" => Value::text(ap),
        };

        let mut widget = Widget::new().with_format(format.clone());

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
                    "toggle_format" => {
                        if let Some(format_alt) = &mut format_alt {
                            std::mem::swap(format_alt, &mut format);
                            break;
                        }
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

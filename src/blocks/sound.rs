//! Volume level
//!
//! This block displays the volume level (according to PulseAudio or ALSA). Right click to toggle mute, scroll to adjust volume.
//!
//! Requires a PulseAudio installation or `alsa-utils` for ALSA.
//!
//! Note that if you are using PulseAudio commands (such as `pactl`) to control your volume, you should select the `"pulseaudio"` (or `"auto"`) driver to see volume changes that exceed 100%.
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
//! ```toml
//! [[block]]
//! block = "sound"
//! format = " $icon $output_description{ $volume|} "
//! ```
//!
//! ```toml
//! [[block]]
//! block = "sound"
//! format = " $icon $output_name{ $volume|} "
//! [block.mappings]
//! "alsa_output.usb-Harman_Multimedia_JBL_Pebbles_1.0.0-00.analog-stereo" = "🔈"
//! "alsa_output.pci-0000_00_1b.0.analog-stereo" = "🎧"
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
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `driver` | `"auto"`, `"pulseaudio"`, `"alsa"`. | `"auto"` (Pulseaudio with ALSA fallback)
//! `format` | A string to customise the output of this block. See below for available placeholders. | <code> $icon {$volume.eng(w:2) &vert;}</code>
//! `name` | PulseAudio device name, or the ALSA control name as found in the output of `amixer -D yourdevice scontrols`. | PulseAudio: `@DEFAULT_SINK@` / ALSA: `Master`
//! `device` | ALSA device name, usually in the form "hw:X" or "hw:X,Y" where `X` is the card number and `Y` is the device number as found in the output of `aplay -l`. | `default`
//! `device_kind` | PulseAudio device kind: `source` or `sink`. | `"sink"`
//! `natural_mapping` | When using the ALSA driver, display the "mapped volume" as given by `alsamixer`/`amixer -M`, which represents the volume level more naturally with respect for the human ear. | `false`
//! `step_width` | The percent volume level is increased/decreased for the selected audio device when scrolling. Capped automatically at 50. | `5`
//! `max_vol` | Max volume in percent that can be set via scrolling. Note it can still be set above this value if changed by another application. | `None`
//! `show_volume_when_muted` | Show the volume even if it is currently muted. | `false`
//! `headphones_indicator` | Change icon when headphones are plugged in (pulseaudio only) | `false`
//! `mappings` | Map `output_name` to custom name. | `None`
//!
//! Placeholder          | Value                             | Type   | Unit
//! ---------------------|-----------------------------------|--------|---------------
//! `icon`               | Icon based on volume              | Icon   | -
//! `volume`             | Current volume. Missing if muted. | Number | %
//! `output_name`        | PulseAudio or ALSA device name    | Text   | -
//! `output_description` | PulseAudio device description, will fallback to `output_name` if no description is available and will be overwritten by mappings (mappings will still use `output_name`) | Text | -
//!
//! Action        | Default button
//! --------------|---------------
//! `toggle_mute` | Rigth
//! `volume_up`   | Wheel Up
//! `volume_down` | Wheel Down
//!
//! #  Icons Used
//!
//! - `microphone_muted`
//! - `microphone_empty` (1 to 20%)
//! - `microphone_half` (21 to 70%)
//! - `microphone_full` (over 71%)
//! - `volume_muted`
//! - `volume_empty` (1 to 20%)
//! - `volume_half` (21 to 70%)
//! - `volume_full` (over 71%)
//! - `headphones`

mod alsa;
#[cfg(feature = "pulseaudio")]
mod pulseaudio;

use super::prelude::*;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(default)]
pub struct Config {
    driver: SoundDriver,
    name: Option<String>,
    device: Option<String>,
    device_kind: DeviceKind,
    natural_mapping: bool,
    #[default(5)]
    step_width: u32,
    format: FormatConfig,
    headphones_indicator: bool,
    show_volume_when_muted: bool,
    mappings: Option<HashMap<String, String>>,
    max_vol: Option<u32>,
}

pub async fn run(config: Config, mut api: CommonApi) -> Result<()> {
    api.set_default_actions(&[
        (MouseButton::Right, None, "toggle_mute"),
        (MouseButton::WheelUp, None, "volume_up"),
        (MouseButton::WheelDown, None, "volume_down"),
    ])
    .await?;

    let mut widget =
        Widget::new().with_format(config.format.with_default(" $icon {$volume.eng(w:2)|} ")?);

    let device_kind = config.device_kind;
    let step_width = config.step_width.clamp(0, 50) as i32;

    let icon = |volume: u32, device: &dyn SoundDevice| -> String {
        if config.headphones_indicator && device_kind == DeviceKind::Sink {
            let headphones = match device.form_factor() {
                // form_factor's possible values are listed at:
                // https://docs.rs/libpulse-binding/2.25.0/libpulse_binding/proplist/properties/constant.DEVICE_FORM_FACTOR.html
                Some("headset") | Some("headphone") | Some("hands-free") | Some("portable") => true,
                // Per discussion at
                // https://github.com/greshake/i3status-rust/pull/1363#issuecomment-1046095869,
                // some sinks may not have the form_factor property, so we should fall back to the
                // active_port if that property is not present.
                None => device
                    .active_port()
                    .map_or(false, |p| p.contains("headphones")),
                // form_factor is present and is some non-headphone value
                _ => false,
            };
            if headphones {
                return "headphones".into();
            }
        }

        format!(
            "{}_{}",
            match device_kind {
                DeviceKind::Source => "microphone",
                DeviceKind::Sink => "volume",
            },
            match volume {
                0 => "muted",
                1..=20 => "empty",
                21..=70 => "half",
                _ => "full",
            }
        )
    };

    type DeviceType = Box<dyn SoundDevice>;
    let mut device: DeviceType = match config.driver {
        SoundDriver::Alsa => Box::new(alsa::Device::new(
            config.name.clone().unwrap_or_else(|| "Master".into()),
            config.device.unwrap_or_else(|| "default".into()),
            config.natural_mapping,
        )?),
        #[cfg(feature = "pulseaudio")]
        SoundDriver::PulseAudio => {
            Box::new(pulseaudio::Device::new(config.device_kind, config.name)?)
        }
        #[cfg(feature = "pulseaudio")]
        SoundDriver::Auto => {
            if let Ok(pulse) = pulseaudio::Device::new(config.device_kind, config.name.clone()) {
                Box::new(pulse)
            } else {
                Box::new(alsa::Device::new(
                    config.name.unwrap_or_else(|| "Master".into()),
                    config.device.unwrap_or_else(|| "default".into()),
                    config.natural_mapping,
                )?)
            }
        }
        #[cfg(not(feature = "pulseaudio"))]
        SoundDriver::Auto => Box::new(alsa::Device::new(
            config.name.clone().unwrap_or_else(|| "Master".into()),
            config.device.unwrap_or_else(|| "default".into()),
            config.natural_mapping,
        )?),
    };

    loop {
        device.get_info().await?;
        let volume = device.volume();

        let mut output_name = device.output_name();
        if let Some(m) = &config.mappings {
            if let Some(mapped) = m.get(&output_name) {
                output_name = mapped.clone();
            }
        }

        let output_description = device
            .output_description()
            .unwrap_or_else(|| output_name.clone());

        let mut values = map! {
            "volume" => Value::percents(volume),
            "output_name" => Value::text(output_name),
            "output_description" => Value::text(output_description),
        };

        if device.muted() {
            values.insert(
                "icon".into(),
                Value::icon(api.get_icon(&icon(0, &*device))?),
            );
            widget.state = State::Warning;
            if !config.show_volume_when_muted {
                values.remove("volume");
            }
        } else {
            values.insert(
                "icon".into(),
                Value::icon(api.get_icon(&icon(volume, &*device))?),
            );
            widget.state = State::Idle;
        }

        widget.set_values(values);
        api.set_widget(&widget).await?;

        loop {
            select! {
                val = device.wait_for_update() => {
                    val?;
                    break;
                }
                event = api.event() => match event {
                    UpdateRequest => break,
                    Action(a) if a == "toggle_mute" => {
                        device.toggle().await?;
                    }
                    Action(a) if a == "volume_up" => {
                        device.set_volume(step_width, config.max_vol).await?;
                    }
                    Action(a) if a == "volume_down" => {
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
enum SoundDriver {
    #[default]
    Auto,
    Alsa,
    #[cfg(feature = "pulseaudio")]
    PulseAudio,
}

#[derive(Deserialize, Debug, SmartDefault, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
enum DeviceKind {
    #[default]
    Sink,
    Source,
}

#[cfg(feature = "pulseaudio")]
impl DeviceKind {
    pub fn default_name(self) -> String {
        match self {
            Self::Sink => pulseaudio::DEFAULT_SINK.lock().unwrap().clone(),
            Self::Source => pulseaudio::DEFAULT_SOURCE.lock().unwrap().clone(),
        }
    }
}

#[async_trait::async_trait]
trait SoundDevice {
    fn volume(&self) -> u32;
    fn muted(&self) -> bool;
    fn output_name(&self) -> String;
    fn output_description(&self) -> Option<String>;
    fn active_port(&self) -> Option<&str>;
    fn form_factor(&self) -> Option<&str>;

    async fn get_info(&mut self) -> Result<()>;
    async fn set_volume(&mut self, step: i32, max_vol: Option<u32>) -> Result<()>;
    async fn toggle(&mut self) -> Result<()>;
    async fn wait_for_update(&mut self) -> Result<()>;
}

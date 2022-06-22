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
//! format = "$output_description{ $volume|}"
//! ```
//!
//! ```toml
//! [[block]]
//! block = "sound"
//! format = "$output_name{ $volume|}"
//! [block.mappings]
//! "alsa_output.usb-Harman_Multimedia_JBL_Pebbles_1.0.0-00.analog-stereo" = "🔈"
//! "alsa_output.pci-0000_00_1b.0.analog-stereo" = "🎧"
//! ```
//!
//! # Configuration
//!
//! Key | Values | Required | Default
//! ----|--------|----------|--------
//! `driver` | `"auto"`, `"pulseaudio"`, `"alsa"`. | No | `"auto"` (Pulseaudio with ALSA fallback)
//! `format` | A string to customise the output of this block. See below for available placeholders. | No | `$volume.eng(2)|`
//! `name` | PulseAudio device name, or the ALSA control name as found in the output of `amixer -D yourdevice scontrols`. | No | PulseAudio: `@DEFAULT_SINK@` / ALSA: `Master`
//! `device` | ALSA device name, usually in the form "hw:X" or "hw:X,Y" where `X` is the card number and `Y` is the device number as found in the output of `aplay -l`. | No | `default`
//! `device_kind` | PulseAudio device kind: `source` or `sink`. | No | `sink`
//! `natural_mapping` | When using the ALSA driver, display the "mapped volume" as given by `alsamixer`/`amixer -M`, which represents the volume level more naturally with respect for the human ear. | No | `false`
//! `step_width` | The percent volume level is increased/decreased for the selected audio device when scrolling. Capped automatically at 50. | No | `5`
//! `max_vol` | Max volume in percent that can be set via scrolling. Note it can still be set above this value if changed by another application. | No | `None`
//! `on_click` | Shell command to run when the sound block is clicked. | No | None
//! `show_volume_when_muted` | Show the volume even if it is currently muted. | No | `false`
//! `headphones_indicator` | Change icon when headphones are plugged in (pulseaudio only) | No | `false`
//!
//!  Key | Value | Type | Unit
//! -----|-------|------|-----
//! `volume` | Current volume. Missing if muted. | Number | %
//! `output_name` | PulseAudio or ALSA device name | Text | -
//! `output_description` | PulseAudio device description, will fallback to `output_name` if no description is available and will be overwritten by mappings (mappings will still use `output_name`) | Text | -
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
#[serde(deny_unknown_fields, default)]
struct SoundConfig {
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

pub async fn run(config: toml::Value, mut api: CommonApi) -> Result<()> {
    let config = SoundConfig::deserialize(config).config_error()?;
    let mut widget = api
        .new_widget()
        .with_format(config.format.with_default("$volume.eng(2)|")?);

    let device_kind = config.device_kind;
    let icon = |volume: u32, headphones: bool| -> String {
        if config.headphones_indicator && headphones && config.device_kind == DeviceKind::Sink {
            "headphones".into()
        } else {
            let mut icon = String::new();
            let _ = write!(
                icon,
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
            );
            icon
        }
    };

    let step_width = config.step_width.clamp(0, 50) as i32;

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

        // TODO: Query port names instead? See https://github.com/greshake/i3status-rust/pull/1363#issue-1069904082
        // Reference: PulseAudio port name definitions are the first item in the well_known_descriptions struct:
        // https://gitlab.freedesktop.org/pulseaudio/pulseaudio/-/blob/0ce3008605e5f644fac4bb5edbb1443110201ec1/src/modules/alsa/alsa-mixer.c#L2709-L2731
        let headphones = device
            .active_port()
            .map(|p| p.contains("headphones") || p.contains("headset"))
            .unwrap_or(false);

        let mut values = map! {
            "volume" => Value::percents(volume),
            "output_name" => Value::text(output_name),
            "output_description" => Value::text(output_description),
        };

        if device.muted() {
            widget.set_icon(&icon(0, headphones))?;
            widget.state = State::Warning;
            if !config.show_volume_when_muted {
                values.remove("volume");
            }
        } else {
            widget.set_icon(&icon(volume, headphones))?;
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
                    Click(click) => match click.button {
                        MouseButton::Right => {
                            device.toggle().await?;
                        }
                        MouseButton::WheelUp => {
                            device.set_volume(step_width, config.max_vol).await?;
                        }
                        MouseButton::WheelDown => {
                            device.set_volume(-step_width, config.max_vol).await?;
                        }
                        _ => ()
                    }
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
    fn active_port(&self) -> Option<String>;

    async fn get_info(&mut self) -> Result<()>;
    async fn set_volume(&mut self, step: i32, max_vol: Option<u32>) -> Result<()>;
    async fn toggle(&mut self) -> Result<()>;
    async fn wait_for_update(&mut self) -> Result<()>;
}

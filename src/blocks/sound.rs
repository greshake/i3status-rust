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

use libpulse_binding::callbacks::ListResult;
use libpulse_binding::context::{
    introspect::ServerInfo, introspect::SinkInfo, introspect::SourceInfo, subscribe::Facility,
    subscribe::InterestMaskSet, subscribe::Operation as SubscribeOperation, Context, FlagSet,
    State as PulseState,
};
use libpulse_binding::mainloop::standard::{IterateResult, Mainloop};
use libpulse_binding::proplist::{properties, Proplist};
use libpulse_binding::volume::{ChannelVolumes, Volume};

use crossbeam_channel::{unbounded, Sender};

use std::cmp::{max, min};
use std::collections::HashMap;
use std::convert::{TryFrom, TryInto};
use std::process::Stdio;
use std::sync::Mutex;
use std::thread;

use tokio::process::{ChildStdout, Command};

use super::prelude::*;

const FILTER: &[char] = &['[', ']', '%'];

#[derive(Deserialize, Debug, Derivative)]
#[serde(deny_unknown_fields, default)]
#[derivative(Default)]
struct SoundConfig {
    driver: SoundDriver,
    name: Option<String>,
    device: Option<String>,
    device_kind: DeviceKind,
    natural_mapping: bool,
    #[derivative(Default(value = "5"))]
    step_width: u32,
    format: FormatConfig,
    headphones_indicator: bool,
    show_volume_when_muted: bool,
    mappings: Option<HashMap<String, String>>,
    max_vol: Option<u32>,
}

pub async fn run(config: toml::Value, mut api: CommonApi) -> Result<()> {
    let mut events = api.get_events().await?;
    let config = SoundConfig::deserialize(config).config_error()?;
    api.set_format(config.format.with_default("$volume.eng(2)|")?);

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
        SoundDriver::Alsa => Box::new(AlsaSoundDevice::new(
            config.name.clone().unwrap_or_else(|| "Master".into()),
            config.device.unwrap_or_else(|| "default".into()),
            config.natural_mapping,
        )?),
        SoundDriver::PulseAudio => {
            Box::new(PulseAudioSoundDevice::new(config.device_kind, config.name)?)
        }
        SoundDriver::Auto => {
            if let Ok(pulse) = PulseAudioSoundDevice::new(config.device_kind, config.name.clone()) {
                Box::new(pulse)
            } else {
                Box::new(AlsaSoundDevice::new(
                    config.name.unwrap_or_else(|| "Master".into()),
                    config.device.unwrap_or_else(|| "default".into()),
                    config.natural_mapping,
                )?)
            }
        }
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
            api.set_icon(&icon(0, headphones))?;
            api.set_state(State::Warning);
            if !config.show_volume_when_muted {
                values.remove("volume");
            }
        } else {
            api.set_icon(&icon(volume, headphones))?;
            api.set_state(State::Idle);
        }

        api.set_values(values);
        api.flush().await?;

        tokio::select! {
            val = device.wait_for_update() => val?,
            Some(BlockEvent::Click(click)) = events.recv() => {
                match click.button {
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

#[derive(Deserialize, Debug, Derivative, Clone, Copy)]
#[serde(rename_all = "lowercase")]
#[derivative(Default)]
enum SoundDriver {
    #[derivative(Default)]
    Auto,
    Alsa,
    PulseAudio,
}

#[derive(Deserialize, Debug, Derivative, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
#[derivative(Default)]
enum DeviceKind {
    #[derivative(Default)]
    Sink,
    Source,
}

impl DeviceKind {
    pub fn default_name(self) -> String {
        match self {
            Self::Sink => PULSEAUDIO_DEFAULT_SINK.lock().unwrap().clone(),
            Self::Source => PULSEAUDIO_DEFAULT_SOURCE.lock().unwrap().clone(),
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

struct AlsaSoundDevice {
    name: String,
    device: String,
    natural_mapping: bool,
    volume: u32,
    muted: bool,

    monitor: ChildStdout,
    buffer: [u8; 2048],
}

impl AlsaSoundDevice {
    fn new(name: String, device: String, natural_mapping: bool) -> Result<Self> {
        Ok(AlsaSoundDevice {
            name,
            device,
            natural_mapping,
            volume: 0,
            muted: false,

            monitor: Command::new("stdbuf")
                .args(&["-oL", "alsactl", "monitor"])
                .stdout(Stdio::piped())
                .spawn()
                .error("Failed to start alsactl monitor")?
                .stdout
                .error("Failed to pipe alsactl monitor output")?,
            buffer: [0; 2048],
        })
    }
}

#[async_trait::async_trait]
impl SoundDevice for AlsaSoundDevice {
    fn volume(&self) -> u32 {
        self.volume
    }

    fn muted(&self) -> bool {
        self.muted
    }

    fn output_name(&self) -> String {
        self.name.clone()
    }

    fn output_description(&self) -> Option<String> {
        // TODO Does Alsa has something similar like descripitons in Pulse?
        None
    }

    fn active_port(&self) -> Option<String> {
        None
    }

    async fn get_info(&mut self) -> Result<()> {
        let mut args = Vec::new();
        if self.natural_mapping {
            args.push("-M")
        };
        args.extend(&["-D", &self.device, "get", &self.name]);

        let output: String = Command::new("amixer")
            .args(&args)
            .output()
            .await
            .map(|o| std::str::from_utf8(&o.stdout).unwrap().trim().into())
            .error("could not run amixer to get sound info")?;

        let last_line = &output.lines().last().error("could not get sound info")?;

        let mut last = last_line
            .split_whitespace()
            .filter(|x| x.starts_with('[') && !x.contains("dB"))
            .map(|s| s.trim_matches(FILTER));

        self.volume = last
            .next()
            .error("could not get volume")?
            .parse::<u32>()
            .error("could not parse volume to u32")?;

        self.muted = last.next().map(|muted| muted == "off").unwrap_or(false);

        Ok(())
    }

    async fn set_volume(&mut self, step: i32, max_vol: Option<u32>) -> Result<()> {
        let new_vol = max(0, self.volume as i32 + step) as u32;
        let capped_volume = if let Some(vol_cap) = max_vol {
            min(new_vol, vol_cap)
        } else {
            new_vol
        };
        let mut args = Vec::new();
        if self.natural_mapping {
            args.push("-M")
        };
        let vol_str = format!("{}%", capped_volume);
        args.extend(&["-D", &self.device, "set", &self.name, &vol_str]);

        Command::new("amixer")
            .args(&args)
            .output()
            .await
            .error("failed to set volume")?;

        self.volume = capped_volume;

        Ok(())
    }

    async fn toggle(&mut self) -> Result<()> {
        let mut args = Vec::new();
        if self.natural_mapping {
            args.push("-M")
        };
        args.extend(&["-D", &self.device, "set", &self.name, "toggle"]);

        Command::new("amixer")
            .args(&args)
            .output()
            .await
            .error("failed to toggle mute")?;

        self.muted = !self.muted;

        Ok(())
    }

    async fn wait_for_update(&mut self) -> Result<()> {
        self.monitor
            .read(&mut self.buffer)
            .await
            .error("Failed to read stdbuf output")
            .map(|_| ())
    }
}

struct PulseAudioConnection {
    mainloop: Mainloop,
    context: Context,
}

struct PulseAudioClient {
    sender: Sender<PulseAudioClientRequest>,
}

struct PulseAudioSoundDevice {
    name: Option<String>,
    description: Option<String>,
    active_port: Option<String>,
    device_kind: DeviceKind,
    volume: Option<ChannelVolumes>,
    volume_avg: u32,
    muted: bool,
    updates: tokio::sync::mpsc::Receiver<()>,
}

#[derive(Debug)]
struct PulseAudioVolInfo {
    volume: ChannelVolumes,
    mute: bool,
    name: String,
    description: Option<String>,
    active_port: Option<String>,
}

impl TryFrom<&SourceInfo<'_>> for PulseAudioVolInfo {
    type Error = ();

    fn try_from(source_info: &SourceInfo) -> std::result::Result<Self, Self::Error> {
        match source_info.name.as_ref() {
            None => Err(()),
            Some(name) => Ok(PulseAudioVolInfo {
                volume: source_info.volume,
                mute: source_info.mute,
                name: name.to_string().into(),
                description: source_info
                    .description
                    .as_ref()
                    .map(|d| d.to_string().into()),
                active_port: source_info
                    .active_port
                    .as_ref()
                    .and_then(|a| a.name.as_ref().map(|n| n.to_string().into())),
            }),
        }
    }
}

impl TryFrom<&SinkInfo<'_>> for PulseAudioVolInfo {
    type Error = ();

    fn try_from(sink_info: &SinkInfo) -> std::result::Result<Self, Self::Error> {
        match sink_info.name.as_ref() {
            None => Err(()),
            Some(name) => Ok(PulseAudioVolInfo {
                volume: sink_info.volume,
                mute: sink_info.mute,
                name: name.to_string().into(),
                description: sink_info.description.as_ref().map(|d| d.to_string().into()),
                active_port: sink_info
                    .active_port
                    .as_ref()
                    .and_then(|a| a.name.as_ref().map(|n| n.to_string().into())),
            }),
        }
    }
}

#[derive(Debug)]
enum PulseAudioClientRequest {
    GetDefaultDevice,
    GetInfoByIndex(DeviceKind, u32),
    GetInfoByName(DeviceKind, String),
    SetVolumeByName(DeviceKind, String, ChannelVolumes),
    SetMuteByName(DeviceKind, String, bool),
}

static PULSEAUDIO_CLIENT: Lazy<Result<PulseAudioClient>> = Lazy::new(PulseAudioClient::new);
static PULSEAUDIO_EVENT_LISTENER: Lazy<Mutex<Vec<tokio::sync::mpsc::Sender<()>>>> =
    Lazy::new(|| Mutex::new(Vec::new()));
static PULSEAUDIO_DEVICES: Lazy<Mutex<HashMap<(DeviceKind, String), PulseAudioVolInfo>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

// Default device names
static PULSEAUDIO_DEFAULT_SOURCE: Lazy<Mutex<String>> =
    Lazy::new(|| Mutex::new("@DEFAULT_SOURCE@".into()));
static PULSEAUDIO_DEFAULT_SINK: Lazy<Mutex<String>> =
    Lazy::new(|| Mutex::new("@DEFAULT_SINK@".into()));

impl PulseAudioConnection {
    fn new() -> Result<Self> {
        let mut proplist = Proplist::new().unwrap();
        proplist
            .set_str(properties::APPLICATION_NAME, env!("CARGO_PKG_NAME"))
            .map_err(|_| Error::new("Could not set pulseaudio APPLICATION_NAME property"))?;

        let mainloop = Mainloop::new().error("Failed to create pulseaudio mainloop")?;

        let mut context = Context::new_with_proplist(
            &mainloop,
            concat!(env!("CARGO_PKG_NAME"), "_context"),
            &proplist,
        )
        .error("Failed to create new pulseaudio context")?;

        context
            .connect(None, FlagSet::NOFLAGS, None)
            .error("Failed to connect to pulseaudio context")?;

        let mut connection = PulseAudioConnection { mainloop, context };

        // Wait for context to be ready
        loop {
            connection.iterate(false)?;
            match connection.context.get_state() {
                PulseState::Ready => {
                    break;
                }
                PulseState::Failed | PulseState::Terminated => {
                    return Err(Error::new("pulseaudio context state failed/terminated"));
                }
                _ => {}
            }
        }

        Ok(connection)
    }

    fn iterate(&mut self, blocking: bool) -> Result<()> {
        match self.mainloop.iterate(blocking) {
            IterateResult::Quit(_) | IterateResult::Err(_) => {
                Err(Error::new("failed to iterate pulseaudio state"))
            }
            IterateResult::Success(_) => Ok(()),
        }
    }
}

impl PulseAudioClient {
    fn new() -> Result<PulseAudioClient> {
        let (send_req, recv_req) = unbounded();
        let (send_result, recv_result) = unbounded();
        let send_result2 = send_result.clone();
        let new_connection = |sender: Sender<Result<()>>| -> PulseAudioConnection {
            let conn = PulseAudioConnection::new();
            match conn {
                Ok(conn) => {
                    sender.send(Ok(())).unwrap();
                    conn
                }
                Err(err) => {
                    sender.send(Err(err)).unwrap();
                    panic!("failed to create pulseaudio connection");
                }
            }
        };

        // requests
        thread::Builder::new()
            .name("sound_pulseaudio_req".into())
            .spawn(move || {
                let mut connection = new_connection(send_result);

                loop {
                    // make sure mainloop dispatched everything
                    loop {
                        connection.iterate(false).unwrap();
                        if connection.context.get_state() == PulseState::Ready {
                            break;
                        }
                    }

                    match recv_req.recv() {
                        Err(_) => {}
                        Ok(req) => {
                            use PulseAudioClientRequest::*;
                            let mut introspector = connection.context.introspect();

                            match req {
                                GetDefaultDevice => {
                                    introspector
                                        .get_server_info(PulseAudioClient::server_info_callback);
                                }
                                GetInfoByIndex(DeviceKind::Sink, index) => {
                                    introspector.get_sink_info_by_index(
                                        index,
                                        PulseAudioClient::sink_info_callback,
                                    );
                                }
                                GetInfoByIndex(DeviceKind::Source, index) => {
                                    introspector.get_source_info_by_index(
                                        index,
                                        PulseAudioClient::source_info_callback,
                                    );
                                }
                                GetInfoByName(DeviceKind::Sink, name) => {
                                    introspector.get_sink_info_by_name(
                                        &name,
                                        PulseAudioClient::sink_info_callback,
                                    );
                                }
                                GetInfoByName(DeviceKind::Source, name) => {
                                    introspector.get_source_info_by_name(
                                        &name,
                                        PulseAudioClient::source_info_callback,
                                    );
                                }
                                SetVolumeByName(DeviceKind::Sink, name, volumes) => {
                                    introspector.set_sink_volume_by_name(&name, &volumes, None);
                                }
                                SetVolumeByName(DeviceKind::Source, name, volumes) => {
                                    introspector.set_source_volume_by_name(&name, &volumes, None);
                                }
                                SetMuteByName(DeviceKind::Sink, name, mute) => {
                                    introspector.set_sink_mute_by_name(&name, mute, None);
                                }
                                SetMuteByName(DeviceKind::Source, name, mute) => {
                                    introspector.set_source_mute_by_name(&name, mute, None);
                                }
                            };

                            // send request and receive response
                            connection.iterate(true).unwrap();
                            connection.iterate(true).unwrap();
                        }
                    }
                }
            })
            .unwrap();
        recv_result
            .recv()
            .error("Failed to receive from pulseaudio thread channel")??;

        // subscribe
        thread::Builder::new()
            .name("sound_pulseaudio_sub".into())
            .spawn(move || {
                let mut connection = new_connection(send_result2);

                // subcribe for events
                connection
                    .context
                    .set_subscribe_callback(Some(Box::new(PulseAudioClient::subscribe_callback)));
                connection.context.subscribe(
                    InterestMaskSet::SERVER | InterestMaskSet::SINK | InterestMaskSet::SOURCE,
                    |_| {},
                );

                connection.mainloop.run().unwrap();
            })
            .unwrap();
        recv_result
            .recv()
            .error("Failed to receive from pulseaudio thread channel")??;

        Ok(PulseAudioClient { sender: send_req })
    }

    fn send(request: PulseAudioClientRequest) -> Result<()> {
        match PULSEAUDIO_CLIENT.as_ref() {
            Ok(client) => {
                client.sender.send(request).unwrap();
                Ok(())
            }
            Err(err) => Err(Error::new(format!(
                "pulseaudio connection failed with error: {}",
                err
            ))),
        }
    }

    fn server_info_callback(server_info: &ServerInfo) {
        if let Some(default_sink) = server_info.default_sink_name.as_ref() {
            *PULSEAUDIO_DEFAULT_SINK.lock().unwrap() = default_sink.to_string().into();
        }

        if let Some(default_source) = server_info.default_source_name.as_ref() {
            *PULSEAUDIO_DEFAULT_SOURCE.lock().unwrap() = default_source.to_string().into();
        }

        PulseAudioClient::send_update_event();
    }

    fn get_info_callback<I: TryInto<PulseAudioVolInfo>>(
        result: ListResult<I>,
    ) -> Option<PulseAudioVolInfo> {
        match result {
            ListResult::End | ListResult::Error => None,
            ListResult::Item(info) => info.try_into().ok(),
        }
    }

    fn sink_info_callback(result: ListResult<&SinkInfo>) {
        if let Some(vol_info) = Self::get_info_callback(result) {
            PULSEAUDIO_DEVICES.lock().unwrap().insert(
                (DeviceKind::Sink, vol_info.name.to_string().into()),
                vol_info,
            );

            PulseAudioClient::send_update_event();
        }
    }

    fn source_info_callback(result: ListResult<&SourceInfo>) {
        if let Some(vol_info) = Self::get_info_callback(result) {
            PULSEAUDIO_DEVICES.lock().unwrap().insert(
                (DeviceKind::Source, vol_info.name.to_string().into()),
                vol_info,
            );

            PulseAudioClient::send_update_event();
        }
    }

    fn subscribe_callback(
        facility: Option<Facility>,
        _operation: Option<SubscribeOperation>,
        index: u32,
    ) {
        match facility {
            None => {}
            Some(facility) => match facility {
                Facility::Server => {
                    PulseAudioClient::send(PulseAudioClientRequest::GetDefaultDevice).ok();
                }
                Facility::Sink => {
                    PulseAudioClient::send(PulseAudioClientRequest::GetInfoByIndex(
                        DeviceKind::Sink,
                        index,
                    ))
                    .ok();
                }
                Facility::Source => {
                    PulseAudioClient::send(PulseAudioClientRequest::GetInfoByIndex(
                        DeviceKind::Source,
                        index,
                    ))
                    .ok();
                }
                _ => {}
            },
        }
    }

    fn send_update_event() {
        for tx in &*PULSEAUDIO_EVENT_LISTENER.lock().unwrap() {
            tx.blocking_send(()).unwrap();
        }
    }
}

impl PulseAudioSoundDevice {
    fn new(device_kind: DeviceKind, name: Option<String>) -> Result<Self> {
        let (tx, rx) = tokio::sync::mpsc::channel(32);
        PULSEAUDIO_EVENT_LISTENER.lock().unwrap().push(tx);

        PulseAudioClient::send(PulseAudioClientRequest::GetDefaultDevice)?;

        let device = PulseAudioSoundDevice {
            name,
            description: None,
            active_port: None,
            device_kind,
            volume: None,
            volume_avg: 0,
            muted: false,
            updates: rx,
        };

        PulseAudioClient::send(PulseAudioClientRequest::GetInfoByName(
            device_kind,
            device.name(),
        ))?;

        Ok(device)
    }

    fn name(&self) -> String {
        self.name
            .clone()
            .unwrap_or_else(|| self.device_kind.default_name())
    }

    fn volume(&mut self, volume: ChannelVolumes) {
        self.volume = Some(volume);
        self.volume_avg = (volume.avg().0 as f32 / Volume::NORMAL.0 as f32 * 100.0).round() as u32;
    }
}

#[async_trait::async_trait]
impl SoundDevice for PulseAudioSoundDevice {
    fn volume(&self) -> u32 {
        self.volume_avg
    }

    fn muted(&self) -> bool {
        self.muted
    }

    fn output_name(&self) -> String {
        self.name()
    }

    fn output_description(&self) -> Option<String> {
        self.description.clone()
    }

    fn active_port(&self) -> Option<String> {
        self.active_port.clone()
    }

    async fn get_info(&mut self) -> Result<()> {
        let devices = PULSEAUDIO_DEVICES.lock().unwrap();

        if let Some(info) = devices.get(&(self.device_kind, self.name())) {
            self.volume(info.volume);
            self.muted = info.mute;
            self.description = info.description.clone();
            self.active_port = info.active_port.clone();
        }

        Ok(())
    }

    async fn set_volume(&mut self, step: i32, max_vol: Option<u32>) -> Result<()> {
        let mut volume = self.volume.error("Volume unknown")?;

        // apply step to volumes
        let step = (step as f32 * Volume::NORMAL.0 as f32 / 100.0).round() as i32;
        for vol in volume.get_mut().iter_mut() {
            let uncapped_vol = max(0, vol.0 as i32 + step) as u32;
            let capped_vol = if let Some(vol_cap) = max_vol {
                min(
                    uncapped_vol,
                    (vol_cap as f32 * Volume::NORMAL.0 as f32 / 100.0).round() as u32,
                )
            } else {
                uncapped_vol
            };
            vol.0 = min(capped_vol, Volume::MAX.0);
        }

        // update volumes
        self.volume(volume);
        PulseAudioClient::send(PulseAudioClientRequest::SetVolumeByName(
            self.device_kind,
            self.name(),
            volume,
        ))?;

        Ok(())
    }

    async fn toggle(&mut self) -> Result<()> {
        self.muted = !self.muted;

        PulseAudioClient::send(PulseAudioClientRequest::SetMuteByName(
            self.device_kind,
            self.name(),
            self.muted,
        ))?;

        Ok(())
    }

    async fn wait_for_update(&mut self) -> Result<()> {
        self.updates
            .recv()
            .await
            .error("Failed to receive new update")
    }
}

use std::cmp::{max, min};
use std::collections::BTreeMap;
use std::io::Read;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use serde_derive::Deserialize;

#[cfg(feature = "pulseaudio")]
use {
    crate::pulse::callbacks::ListResult,
    crate::pulse::context::{
        introspect::ServerInfo, introspect::SinkInfo, introspect::SourceInfo, subscribe::Facility,
        subscribe::InterestMaskSet, subscribe::Operation as SubscribeOperation, Context, FlagSet,
        State as PulseState,
    },
    crate::pulse::mainloop::standard::IterateResult,
    crate::pulse::mainloop::standard::Mainloop,
    crate::pulse::proplist::properties::DEVICE_FORM_FACTOR,
    crate::pulse::proplist::{properties, Proplist},
    crate::pulse::volume::{ChannelVolumes, Volume},
    crossbeam_channel::unbounded,
    lazy_static::lazy_static,
    std::cell::RefCell,
    std::collections::HashMap,
    std::convert::{TryFrom, TryInto},
    std::ops::Deref,
    std::rc::Rc,
    std::sync::Mutex,
};

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::SharedConfig;
use crate::config::{LogicalDirection, Scrolling};
use crate::errors::*;
use crate::formatting::value::Value;
use crate::formatting::FormatTemplate;
use crate::protocol::i3bar_event::{I3BarEvent, MouseButton};
use crate::scheduler::Task;
use crate::subprocess::spawn_child_async;
use crate::widgets::text::TextWidget;
use crate::widgets::{I3BarWidget, State};

trait SoundDevice {
    fn volume(&self) -> u32;
    fn muted(&self) -> bool;
    fn output_name(&self) -> String;
    fn output_description(&self) -> Option<String>;
    fn active_port(&self) -> Option<&str>;
    fn form_factor(&self) -> Option<&str>;

    fn get_info(&mut self) -> Result<()>;
    fn set_volume(&mut self, step: i32, max_vol: Option<u32>) -> Result<()>;
    fn toggle(&mut self) -> Result<()>;
    fn monitor(&mut self, id: usize, tx_update_request: Sender<Task>) -> Result<()>;
}

struct AlsaSoundDevice {
    name: String,
    device: String,
    natural_mapping: bool,
    volume: u32,
    muted: bool,
}

impl AlsaSoundDevice {
    fn new(name: String, device: String, natural_mapping: bool) -> Result<Self> {
        let mut sd = AlsaSoundDevice {
            name,
            device,
            natural_mapping,
            volume: 0,
            muted: false,
        };
        sd.get_info()?;

        Ok(sd)
    }
}

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
    fn active_port(&self) -> Option<&str> {
        None
    }
    fn form_factor(&self) -> Option<&str> {
        None
    }

    fn get_info(&mut self) -> Result<()> {
        let mut args = Vec::new();
        if self.natural_mapping {
            args.push("-M")
        };
        args.extend(&["-D", &self.device, "get", &self.name]);

        let output = Command::new("amixer")
            .args(&args)
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
            .block_error("sound", "could not run amixer to get sound info")?;

        let last_line = &output
            .lines()
            .last()
            .block_error("sound", "could not get sound info")?;

        let last = last_line
            .split_whitespace()
            .filter(|x| x.starts_with('[') && !x.contains("dB"))
            .map(|s| s.trim_matches(FILTER))
            .collect::<Vec<&str>>();

        self.volume = last
            .get(0)
            .block_error("sound", "could not get volume")?
            .parse::<u32>()
            .block_error("sound", "could not parse volume to u32")?;

        self.muted = last.get(1).map(|muted| *muted == "off").unwrap_or(false);

        Ok(())
    }

    fn set_volume(&mut self, step: i32, max_vol: Option<u32>) -> Result<()> {
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
        let vol_str = &format!("{}%", capped_volume);
        args.extend(&["-D", &self.device, "set", &self.name, vol_str]);

        Command::new("amixer")
            .args(&args)
            .output()
            .block_error("sound", "failed to set volume")?;

        self.volume = capped_volume;

        Ok(())
    }

    fn toggle(&mut self) -> Result<()> {
        let mut args = Vec::new();
        if self.natural_mapping {
            args.push("-M")
        };
        args.extend(&["-D", &self.device, "set", &self.name, "toggle"]);

        Command::new("amixer")
            .args(&args)
            .output()
            .block_error("sound", "failed to toggle mute")?;

        self.muted = !self.muted;

        Ok(())
    }

    fn monitor(&mut self, id: usize, tx_update_request: Sender<Task>) -> Result<()> {
        // Monitor volume changes in a separate thread.
        thread::Builder::new()
            .name("sound_alsa".into())
            .spawn(move || {
                // Line-buffer to reduce noise.
                let mut monitor = Command::new("stdbuf")
                    .args(&["-oL", "alsactl", "monitor"])
                    .stdout(Stdio::piped())
                    .spawn()
                    .expect("Failed to start alsactl monitor")
                    .stdout
                    .expect("Failed to pipe alsactl monitor output");

                let mut buffer = [0; 1024]; // Should be more than enough.
                loop {
                    // Block until we get some output. Doesn't really matter what
                    // the output actually is -- these are events -- we just update
                    // the sound information if *something* happens.
                    if monitor.read(&mut buffer).is_ok() {
                        tx_update_request
                            .send(Task {
                                id,
                                update_time: Instant::now(),
                            })
                            .unwrap();
                    }
                    // Don't update too often. Wait 1/4 second, fast enough for
                    // volume button mashing but slow enough to skip event spam.
                    thread::sleep(Duration::new(0, 250_000_000))
                }
            })
            .unwrap();

        Ok(())
    }
}

#[cfg(feature = "pulseaudio")]
struct PulseAudioConnection {
    mainloop: Rc<RefCell<Mainloop>>,
    context: Rc<RefCell<Context>>,
}

#[cfg(feature = "pulseaudio")]
struct PulseAudioClient {
    sender: Sender<PulseAudioClientRequest>,
}

#[cfg(feature = "pulseaudio")]
struct PulseAudioSoundDevice {
    name: Option<String>,
    description: Option<String>,
    active_port: Option<String>,
    form_factor: Option<String>,
    device_kind: DeviceKind,
    volume: Option<ChannelVolumes>,
    volume_avg: u32,
    muted: bool,
}

#[cfg(feature = "pulseaudio")]
#[derive(Debug)]
struct PulseAudioVolInfo {
    volume: ChannelVolumes,
    mute: bool,
    name: String,
    description: Option<String>,
    active_port: Option<String>,
    form_factor: Option<String>,
}

#[cfg(feature = "pulseaudio")]
impl TryFrom<&SourceInfo<'_>> for PulseAudioVolInfo {
    type Error = ();

    fn try_from(source_info: &SourceInfo) -> std::result::Result<Self, Self::Error> {
        match source_info.name.as_ref() {
            None => Err(()),
            Some(name) => Ok(PulseAudioVolInfo {
                volume: source_info.volume,
                mute: source_info.mute,
                name: name.to_string(),
                description: source_info
                    .description
                    .clone()
                    .map(|description| description.into_owned()),
                active_port: source_info
                    .active_port
                    .as_ref()
                    .and_then(|a| a.name.as_ref().map(|n| n.to_string())),
                form_factor: source_info.proplist.get_str(DEVICE_FORM_FACTOR),
            }),
        }
    }
}

#[cfg(feature = "pulseaudio")]
impl TryFrom<&SinkInfo<'_>> for PulseAudioVolInfo {
    type Error = ();

    fn try_from(sink_info: &SinkInfo) -> std::result::Result<Self, Self::Error> {
        match sink_info.name.as_ref() {
            None => Err(()),
            Some(name) => Ok(PulseAudioVolInfo {
                volume: sink_info.volume,
                mute: sink_info.mute,
                name: name.to_string(),
                description: sink_info
                    .description
                    .clone()
                    .map(|description| description.into_owned()),
                active_port: sink_info
                    .active_port
                    .as_ref()
                    .and_then(|a| a.name.as_ref().map(|n| n.to_string())),
                form_factor: sink_info.proplist.get_str(DEVICE_FORM_FACTOR),
            }),
        }
    }
}

#[cfg(feature = "pulseaudio")]
#[derive(Debug)]
enum PulseAudioClientRequest {
    GetDefaultDevice,
    GetInfoByIndex(DeviceKind, u32),
    GetInfoByName(DeviceKind, String),
    SetVolumeByName(DeviceKind, String, ChannelVolumes),
    SetMuteByName(DeviceKind, String, bool),
}

#[cfg(feature = "pulseaudio")]
lazy_static! {
    static ref PULSEAUDIO_CLIENT: Result<PulseAudioClient> = PulseAudioClient::new();
    static ref PULSEAUDIO_EVENT_LISTENER: Mutex<HashMap<usize, Sender<Task>>> =
        Mutex::new(HashMap::new());

    // Default device names
    static ref PULSEAUDIO_DEFAULT_SOURCE: Mutex<String> = Mutex::new("@DEFAULT_SOURCE@".into());
    static ref PULSEAUDIO_DEFAULT_SINK: Mutex<String> = Mutex::new("@DEFAULT_SINK@".into());

    // State for each device
    static ref PULSEAUDIO_DEVICES: Mutex<HashMap<(DeviceKind, String), PulseAudioVolInfo>> =
        Mutex::new(HashMap::new());
}

#[cfg(feature = "pulseaudio")]
impl PulseAudioConnection {
    fn new() -> Result<Self> {
        let mut proplist = Proplist::new().unwrap();
        proplist
            .set_str(properties::APPLICATION_NAME, "i3status-rs")
            .block_error(
                "sound",
                "could not set pulseaudio APPLICATION_NAME property",
            )?;

        let mainloop = Rc::new(RefCell::new(
            Mainloop::new().block_error("sound", "failed to create pulseaudio mainloop")?,
        ));

        let context = Rc::new(RefCell::new(
            Context::new_with_proplist(mainloop.borrow().deref(), "i3status-rs_context", &proplist)
                .block_error("sound", "failed to create new pulseaudio context")?,
        ));

        context
            .borrow_mut()
            .connect(None, FlagSet::NOFLAGS, None)
            .block_error("sound", "failed to connect to pulseaudio context")?;

        let mut connection = PulseAudioConnection { mainloop, context };

        // Wait for context to be ready
        loop {
            connection.iterate(false)?;
            match connection.context.borrow().get_state() {
                PulseState::Ready => {
                    break;
                }
                PulseState::Failed | PulseState::Terminated => {
                    return Err(BlockError(
                        "sound".into(),
                        "pulseaudio context state failed/terminated".into(),
                    ))
                }
                _ => {}
            }
        }

        Ok(connection)
    }

    fn iterate(&mut self, blocking: bool) -> Result<()> {
        match self.mainloop.borrow_mut().iterate(blocking) {
            IterateResult::Quit(_) | IterateResult::Err(_) => Err(BlockError(
                "sound".into(),
                "failed to iterate pulseaudio state".into(),
            )),
            IterateResult::Success(_) => Ok(()),
        }
    }
}

#[cfg(feature = "pulseaudio")]
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
        let thread_result = || -> Result<()> {
            recv_result
                .recv()
                .block_error("sound", "failed to receive from pulseaudio thread channel")?
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
                        if connection.context.borrow().get_state() == PulseState::Ready {
                            break;
                        }
                    }

                    match recv_req.recv() {
                        Err(_) => {}
                        Ok(req) => {
                            use PulseAudioClientRequest::*;
                            let mut introspector = connection.context.borrow_mut().introspect();

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
        thread_result()?;

        // subscribe
        thread::Builder::new()
            .name("sound_pulseaudio_sub".into())
            .spawn(move || {
                let connection = new_connection(send_result2);

                // subcribe for events
                connection
                    .context
                    .borrow_mut()
                    .set_subscribe_callback(Some(Box::new(PulseAudioClient::subscribe_callback)));
                connection.context.borrow_mut().subscribe(
                    InterestMaskSet::SERVER | InterestMaskSet::SINK | InterestMaskSet::SOURCE,
                    |_| {},
                );

                connection.mainloop.borrow_mut().run().unwrap();
            })
            .unwrap();
        thread_result()?;

        Ok(PulseAudioClient { sender: send_req })
    }

    fn send(request: PulseAudioClientRequest) -> Result<()> {
        match PULSEAUDIO_CLIENT.as_ref() {
            Ok(client) => {
                client.sender.send(request).unwrap();
                Ok(())
            }
            Err(err) => Err(BlockError(
                "sound".into(),
                format!("pulseaudio connection failed with error: {}", err),
            )),
        }
    }

    fn server_info_callback(server_info: &ServerInfo) {
        if let Some(default_sink) = server_info.default_sink_name.as_ref() {
            *PULSEAUDIO_DEFAULT_SINK.lock().unwrap() = default_sink.to_string();
        }

        if let Some(default_source) = server_info.default_source_name.as_ref() {
            *PULSEAUDIO_DEFAULT_SOURCE.lock().unwrap() = default_source.to_string();
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
            PULSEAUDIO_DEVICES
                .lock()
                .unwrap()
                .insert((DeviceKind::Sink, vol_info.name.to_string()), vol_info);

            PulseAudioClient::send_update_event();
        }
    }

    fn source_info_callback(result: ListResult<&SourceInfo>) {
        if let Some(vol_info) = Self::get_info_callback(result) {
            PULSEAUDIO_DEVICES
                .lock()
                .unwrap()
                .insert((DeviceKind::Source, vol_info.name.to_string()), vol_info);

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
        for (id, tx_update_request) in &*PULSEAUDIO_EVENT_LISTENER.lock().unwrap() {
            tx_update_request
                .send(Task {
                    id: *id,
                    update_time: Instant::now(),
                })
                .unwrap();
        }
    }
}

#[cfg(feature = "pulseaudio")]
impl PulseAudioSoundDevice {
    fn new(device_kind: DeviceKind, name: Option<String>) -> Result<Self> {
        PulseAudioClient::send(PulseAudioClientRequest::GetDefaultDevice)?;

        let device = PulseAudioSoundDevice {
            name,
            description: None,
            active_port: None,
            form_factor: None,
            device_kind,
            volume: None,
            volume_avg: 0,
            muted: false,
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

#[cfg(feature = "pulseaudio")]
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

    fn active_port(&self) -> Option<&str> {
        self.active_port.as_deref()
    }

    fn form_factor(&self) -> Option<&str> {
        self.form_factor.as_deref()
    }

    fn get_info(&mut self) -> Result<()> {
        let devices = PULSEAUDIO_DEVICES.lock().unwrap();

        if let Some(info) = devices.get(&(self.device_kind, self.name())) {
            self.volume(info.volume);
            self.muted = info.mute;
            self.description = info.description.clone();
            self.active_port = info.active_port.clone();
            self.form_factor = info.form_factor.clone();
        }

        Ok(())
    }

    fn set_volume(&mut self, step: i32, max_vol: Option<u32>) -> Result<()> {
        let mut volume = self.volume.block_error("sound", "volume unknown")?;

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

    fn toggle(&mut self) -> Result<()> {
        self.muted = !self.muted;

        PulseAudioClient::send(PulseAudioClientRequest::SetMuteByName(
            self.device_kind,
            self.name(),
            self.muted,
        ))?;

        Ok(())
    }

    fn monitor(&mut self, id: usize, tx_update_request: Sender<Task>) -> Result<()> {
        PULSEAUDIO_EVENT_LISTENER
            .lock()
            .unwrap()
            .insert(id, tx_update_request);
        Ok(())
    }
}

// TODO: Use the alsa control bindings to implement push updates
pub struct Sound {
    text: TextWidget,
    id: usize,
    device: Box<dyn SoundDevice>,
    device_kind: DeviceKind,
    step_width: u32,
    format: FormatTemplate,
    headphones_indicator: bool,
    on_click: Option<String>,
    show_volume_when_muted: bool,
    mappings: Option<BTreeMap<String, String>>,
    max_vol: Option<u32>,
    scrolling: Scrolling,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DeviceKind {
    Sink,
    Source,
}

#[cfg(feature = "pulseaudio")]
impl DeviceKind {
    pub fn default_name(self) -> String {
        match self {
            Self::Sink => PULSEAUDIO_DEFAULT_SINK.lock().unwrap().to_string(),
            Self::Source => PULSEAUDIO_DEFAULT_SOURCE.lock().unwrap().to_string(),
        }
    }
}

impl Default for DeviceKind {
    fn default() -> Self {
        Self::Sink
    }
}

#[derive(Deserialize, Copy, Clone, Debug)]
#[serde(rename_all = "lowercase")]
pub enum SoundDriver {
    Auto,
    Alsa,
    #[cfg(feature = "pulseaudio")]
    PulseAudio,
}

impl Default for SoundDriver {
    fn default() -> Self {
        SoundDriver::Auto
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct SoundConfig {
    /// ALSA / PulseAudio sound device name
    pub driver: SoundDriver,

    /// PulseAudio device name, or
    /// ALSA control name as listed in the output of `amixer -D yourdevice scontrols` (default is "Master")
    pub name: Option<String>,

    /// ALSA device name, usually in the form "hw:#" where # is the number of the card desired (default is "default")
    pub device: Option<String>,

    /// Type of device: sink or source (default is "sink")
    pub device_kind: DeviceKind,

    /// Use the mapped volume for evaluating the percentage representation like alsamixer, to be more natural for human ear
    pub natural_mapping: bool,

    /// The steps volume is in/decreased for the selected audio device (When greater than 50 it gets limited to 50)
    pub step_width: u32,

    /// Format string for displaying sound information.
    /// placeholders: {volume}
    pub format: FormatTemplate,

    /// Change icon when headphones are plugged in (pulseaudio only)
    pub headphones_indicator: bool,

    pub show_volume_when_muted: bool,

    pub mappings: Option<BTreeMap<String, String>>,

    pub max_vol: Option<u32>,
}

impl Default for SoundConfig {
    fn default() -> Self {
        Self {
            driver: Default::default(),
            name: None,
            device: None,
            device_kind: Default::default(),
            natural_mapping: false,
            step_width: 5,
            format: FormatTemplate::default(),
            headphones_indicator: false,
            show_volume_when_muted: false,
            mappings: None,
            max_vol: None,
        }
    }
}

impl Sound {
    fn icon(&self, volume: u32) -> String {
        if self.headphones_indicator && self.device_kind == DeviceKind::Sink {
            let headphones = match self.device.form_factor() {
                // form_factor's possible values are listed at:
                // https://docs.rs/libpulse-binding/2.25.0/libpulse_binding/proplist/properties/constant.DEVICE_FORM_FACTOR.html
                Some("headset") | Some("headphone") | Some("hands-free") | Some("portable") => true,
                // Per discussion at
                // https://github.com/greshake/i3status-rust/pull/1363#issuecomment-1046095869,
                // some sinks may not have the form_factor property, so we should fall back to the
                // active_port if that property is not present.
                None => self
                    .device
                    .active_port()
                    .map_or(false, |p| p.contains("headphones")),
                // form_factor is present and is some non-headphone value
                _ => false,
            };
            if headphones {
                return String::from("headphones");
            }
        }

        let prefix = match self.device_kind {
            DeviceKind::Source => "microphone",
            DeviceKind::Sink => "volume",
        };

        let suffix = match volume {
            0 => "muted",
            1..=20 => "empty",
            21..=70 => "half",
            _ => "full",
        };

        format!("{}_{}", prefix, suffix)
    }
}

impl ConfigBlock for Sound {
    type Config = SoundConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        shared_config: SharedConfig,
        tx_update_request: Sender<Task>,
    ) -> Result<Self> {
        let mut step_width = block_config.step_width;
        if step_width > 50 {
            step_width = 50;
        }

        #[cfg(not(feature = "pulseaudio"))]
        type PulseAudioSoundDevice = AlsaSoundDevice;

        // try to create a pulseaudio device if feature is enabled and `driver != "alsa"`
        let pulseaudio_device: Result<PulseAudioSoundDevice> = match block_config.driver {
            #[cfg(feature = "pulseaudio")]
            SoundDriver::Auto | SoundDriver::PulseAudio => {
                PulseAudioSoundDevice::new(block_config.device_kind, block_config.name.clone())
            }
            _ => Err(BlockError(
                "sound".into(),
                "PulseAudio feature or driver disabled".into(),
            )),
        };

        // prefer PulseAudio if available and selected, fallback to ALSA
        let device: Box<dyn SoundDevice> = match pulseaudio_device {
            Ok(dev) => Box::new(dev),
            Err(_) => Box::new(AlsaSoundDevice::new(
                block_config.name.unwrap_or_else(|| "Master".into()),
                block_config.device.unwrap_or_else(|| "default".into()),
                block_config.natural_mapping,
            )?),
        };

        let mut sound = Self {
            id,
            device,
            device_kind: block_config.device_kind,
            format: block_config.format.with_default("{volume}")?,
            headphones_indicator: block_config.headphones_indicator,
            step_width,
            on_click: None,
            show_volume_when_muted: block_config.show_volume_when_muted,
            mappings: block_config.mappings,
            max_vol: block_config.max_vol,
            scrolling: shared_config.scrolling,
            text: TextWidget::new(id, 0, shared_config).with_icon("volume_empty")?,
        };

        sound.device.monitor(id, tx_update_request)?;

        Ok(sound)
    }

    fn override_on_click(&mut self) -> Option<&mut Option<String>> {
        Some(&mut self.on_click)
    }
}

// To filter [100%] output from amixer into 100
const FILTER: &[char] = &['[', ']', '%'];

impl Block for Sound {
    fn update(&mut self) -> Result<Option<Update>> {
        self.device.get_info()?;

        let volume = self.device.volume();
        let (output_name, output_description) = {
            let mut output_name = self.device.output_name();
            let mut output_description = self
                .device
                .output_description()
                .unwrap_or_else(|| output_name.clone());

            if let Some(mapped_name) = if let Some(m) = &self.mappings {
                m.get(&output_name)
                    .map(|output_name| output_name.to_string())
            } else {
                None
            } {
                output_name = mapped_name.clone();
                output_description = mapped_name;
            }

            (output_name, output_description)
        };

        let values = map!(
            "volume" => Value::from_integer(volume as i64).percents(),
            "output_name" => Value::from_string(output_name),
            "output_description" => Value::from_string(output_description),
        );
        let texts = self.format.render(&values)?;

        if self.device.muted() {
            self.text.set_icon(&self.icon(0))?;
            if self.show_volume_when_muted {
                self.text.set_texts(texts);
            } else {
                self.text.set_text(String::new());
            }
            self.text.set_state(State::Warning);
        } else {
            self.text.set_icon(&self.icon(volume))?;
            self.text.set_state(State::Idle);
            self.text.set_texts(texts);
        }

        Ok(None)
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.text]
    }

    fn click(&mut self, e: &I3BarEvent) -> Result<()> {
        match e.button {
            MouseButton::Right => self.device.toggle()?,
            MouseButton::Left => {
                if let Some(ref cmd) = self.on_click {
                    spawn_child_async("sh", &["-c", cmd])
                        .block_error("sound", "could not spawn child")?;
                }
            }
            _ => {
                use LogicalDirection::*;
                match self.scrolling.to_logical_direction(e.button) {
                    Some(Up) => self
                        .device
                        .set_volume(self.step_width as i32, self.max_vol)?,
                    Some(Down) => self
                        .device
                        .set_volume(-(self.step_width as i32), self.max_vol)?,
                    None => (),
                }
            }
        }
        self.update()?;
        Ok(())
    }

    fn id(&self) -> usize {
        self.id
    }
}

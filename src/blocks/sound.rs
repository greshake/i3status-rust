#[cfg(feature = "pulseaudio")]
use {
    crate::pulse::callbacks::ListResult,
    crate::pulse::context::{
        flags, introspect::ServerInfo, introspect::SinkInfo, introspect::SourceInfo,
        subscribe::subscription_masks, subscribe::Facility,
        subscribe::Operation as SubscribeOperation, Context, State as PulseState,
    },
    crate::pulse::mainloop::standard::IterateResult,
    crate::pulse::mainloop::standard::Mainloop,
    crate::pulse::proplist::{properties, Proplist},
    crate::pulse::volume::{ChannelVolumes, VOLUME_MAX, VOLUME_NORM},
    crossbeam_channel::unbounded,
    lazy_static::lazy_static,
    std::cell::RefCell,
    std::cmp::min,
    std::collections::HashMap,
    std::convert::{TryFrom, TryInto},
    std::ops::Deref,
    std::rc::Rc,
    std::sync::Mutex,
};

use std::cmp::max;
use std::collections::BTreeMap;
use std::io::Read;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use serde_derive::Deserialize;
use uuid::Uuid;

use crate::blocks::Update;
use crate::blocks::{Block, ConfigBlock};
use crate::config::{Config, LogicalDirection};
use crate::errors::*;
use crate::input::{I3BarEvent, MouseButton};
use crate::scheduler::Task;
use crate::subprocess::spawn_child_async;
use crate::util::{format_percent_bar, FormatTemplate};
use crate::widget::{I3BarWidget, State};
use crate::widgets::button::ButtonWidget;

trait SoundDevice {
    fn volume(&self) -> u32;
    fn muted(&self) -> bool;
    fn output_name(&self) -> String;

    fn get_info(&mut self) -> Result<()>;
    fn set_volume(&mut self, step: i32) -> Result<()>;
    fn toggle(&mut self) -> Result<()>;
    fn monitor(&mut self, id: String, tx_update_request: Sender<Task>) -> Result<()>;
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

    fn set_volume(&mut self, step: i32) -> Result<()> {
        let volume = max(0, self.volume as i32 + step) as u32;

        let mut args = Vec::new();
        if self.natural_mapping {
            args.push("-M")
        };
        let vol_str = &format!("{}%", volume);
        args.extend(&["-D", &self.device, "set", &self.name, &vol_str]);

        Command::new("amixer")
            .args(&args)
            .output()
            .block_error("sound", "failed to set volume")?;

        self.volume = volume;

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

    fn monitor(&mut self, id: String, tx_update_request: Sender<Task>) -> Result<()> {
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
                                id: id.clone(),
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
    static ref PULSEAUDIO_EVENT_LISTENER: Mutex<HashMap<String, Sender<Task>>> =
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
            .connect(None, flags::NOFLAGS, None)
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
            match recv_result.recv() {
                Err(_) => Err(BlockError(
                    "sound".into(),
                    "failed to receive from pulseaudio thread channel".into(),
                )),
                Ok(result) => result,
            }
        };

        // requests
        thread::Builder::new()
            .name("sound_pulseaudio_req".into())
            .spawn(move || {
                let mut connection = new_connection(send_result);

                loop {
                    // make sure mainloop dispatched everything
                    for _ in 0..10 {
                        connection.iterate(false).unwrap();
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
                    subscription_masks::SERVER
                        | subscription_masks::SINK
                        | subscription_masks::SOURCE,
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
                    id: id.clone(),
                    update_time: Instant::now(),
                })
                .unwrap();
        }
    }
}

#[cfg(feature = "pulseaudio")]
impl PulseAudioSoundDevice {
    fn new(device_kind: DeviceKind) -> Result<Self> {
        PulseAudioClient::send(PulseAudioClientRequest::GetDefaultDevice)?;

        let device = PulseAudioSoundDevice {
            name: None,
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

    fn with_name(mut self, name: String) -> Self {
        self.name = Some(name);
        self
    }

    fn name(&self) -> String {
        self.name
            .clone()
            .unwrap_or_else(|| self.device_kind.default_name())
    }

    fn volume(&mut self, volume: ChannelVolumes) {
        self.volume = Some(volume);
        self.volume_avg = (volume.avg().0 as f32 / VOLUME_NORM.0 as f32 * 100.0).round() as u32;
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

    fn get_info(&mut self) -> Result<()> {
        let devices = PULSEAUDIO_DEVICES.lock().unwrap();

        if let Some(info) = devices.get(&(self.device_kind, self.name())) {
            self.volume(info.volume);
            self.muted = info.mute;
        }

        Ok(())
    }

    fn set_volume(&mut self, step: i32) -> Result<()> {
        let mut volume = match self.volume {
            Some(volume) => volume,
            None => return Err(BlockError("sound".into(), "volume unknown".into())),
        };

        // apply step to volumes
        let step = (step as f32 * VOLUME_NORM.0 as f32 / 100.0).round() as i32;
        for vol in volume.get_mut().iter_mut() {
            vol.0 = min(max(0, vol.0 as i32 + step) as u32, VOLUME_MAX.0);
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

    fn monitor(&mut self, id: String, tx_update_request: Sender<Task>) -> Result<()> {
        PULSEAUDIO_EVENT_LISTENER
            .lock()
            .unwrap()
            .insert(id, tx_update_request);
        Ok(())
    }
}

// TODO: Use the alsa control bindings to implement push updates
pub struct Sound {
    text: ButtonWidget,
    id: String,
    device: Box<dyn SoundDevice>,
    device_kind: DeviceKind,
    step_width: u32,
    format: FormatTemplate,
    config: Config,
    on_click: Option<String>,
    show_volume_when_muted: bool,
    bar: bool,
    mappings: Option<BTreeMap<String, String>>,
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

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct SoundConfig {
    /// ALSA / PulseAudio sound device name
    #[serde(default)]
    pub driver: SoundDriver,

    /// PulseAudio device name, or
    /// ALSA control name as listed in the output of `amixer -D yourdevice scontrols` (default is "Master")
    #[serde(default = "SoundConfig::default_name")]
    pub name: Option<String>,

    /// ALSA device name, usually in the form "hw:#" where # is the number of the card desired (default is "default")
    #[serde(default = "SoundConfig::default_device")]
    pub device: Option<String>,

    /// Type of device: sink or source (default is "sink")
    #[serde(default)]
    pub device_kind: DeviceKind,

    /// Use the mapped volume for evaluating the percentage representation like alsamixer, to be more natural for human ear
    #[serde(default = "SoundConfig::default_natural_mapping")]
    pub natural_mapping: bool,

    /// The steps volume is in/decreased for the selected audio device (When greater than 50 it gets limited to 50)
    #[serde(default = "SoundConfig::default_step_width")]
    pub step_width: u32,

    /// Format string for displaying sound information.
    /// placeholders: {volume}
    #[serde(default = "SoundConfig::default_format")]
    pub format: String,

    #[serde(default = "SoundConfig::default_on_click")]
    pub on_click: Option<String>,

    #[serde(default = "SoundConfig::default_show_volume_when_muted")]
    pub show_volume_when_muted: bool,

    /// Show volume as bar instead of percent
    #[serde(default = "SoundConfig::default_bar")]
    pub bar: bool,

    #[serde(default = "SoundConfig::default_mappings")]
    pub mappings: Option<BTreeMap<String, String>>,
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

impl SoundConfig {
    fn default_name() -> Option<String> {
        None
    }

    fn default_device() -> Option<String> {
        None
    }

    fn default_natural_mapping() -> bool {
        false
    }

    fn default_step_width() -> u32 {
        5
    }

    fn default_format() -> String {
        "{volume}%".into()
    }

    fn default_on_click() -> Option<String> {
        None
    }

    fn default_show_volume_when_muted() -> bool {
        false
    }

    fn default_bar() -> bool {
        false
    }

    fn default_mappings() -> Option<BTreeMap<String, String>> {
        None
    }
}

impl Sound {
    fn icon(&self, volume: u32) -> String {
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

    fn display(&mut self) -> Result<()> {
        self.device.get_info()?;

        let volume = self.device.volume();
        let output_name = self.device.output_name();
        let mapped_output_name = if let Some(m) = &self.mappings {
            match m.get(&output_name) {
                Some(mapping) => mapping.to_string(),
                None => output_name,
            }
        } else {
            output_name
        };
        let values = map!("{volume}" => format!("{:02}", volume),
                          "{output_name}" => mapped_output_name
        );
        let text = self.format.render_static_str(&values)?;

        if self.device.muted() {
            self.text.set_icon(&self.icon(0));
            if self.show_volume_when_muted {
                if self.bar {
                    self.text.set_text(format_percent_bar(volume as f32));
                } else {
                    self.text.set_text(text);
                }
            } else {
                self.text.set_text("");
            }
            self.text.set_state(State::Warning);
        } else {
            self.text.set_icon(&self.icon(volume));
            self.text.set_text(if self.bar {
                format_percent_bar(volume as f32)
            } else {
                text
            });
            self.text.set_state(State::Idle);
        }

        Ok(())
    }
}

impl ConfigBlock for Sound {
    type Config = SoundConfig;

    fn new(
        block_config: Self::Config,
        config: Config,
        tx_update_request: Sender<Task>,
    ) -> Result<Self> {
        let id = Uuid::new_v4().to_simple().to_string();
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
                let sound_device = PulseAudioSoundDevice::new(block_config.device_kind);

                match block_config.name.as_ref() {
                    None => sound_device,
                    Some(name) => sound_device.map(|device| device.with_name(name.to_string())),
                }
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
            text: ButtonWidget::new(config.clone(), &id).with_icon("volume_empty"),
            id: id.clone(),
            device,
            device_kind: block_config.device_kind,
            format: FormatTemplate::from_string(&block_config.format)?,
            step_width,
            config,
            on_click: block_config.on_click,
            show_volume_when_muted: block_config.show_volume_when_muted,
            bar: block_config.bar,
            mappings: block_config.mappings,
        };

        sound.device.monitor(id, tx_update_request)?;

        Ok(sound)
    }
}

// To filter [100%] output from amixer into 100
const FILTER: &[char] = &['[', ']', '%'];

impl Block for Sound {
    fn update(&mut self) -> Result<Option<Update>> {
        self.display()?;
        Ok(None)
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.text]
    }

    fn click(&mut self, e: &I3BarEvent) -> Result<()> {
        if let Some(ref name) = e.name {
            if name.as_str() == self.id {
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
                        match self.config.scrolling.to_logical_direction(e.button) {
                            Some(Up) => self.device.set_volume(self.step_width as i32)?,
                            Some(Down) => self.device.set_volume(-(self.step_width as i32))?,
                            None => (),
                        }
                    }
                }
                self.display()?;
            }
        }

        Ok(())
    }

    fn id(&self) -> &str {
        &self.id
    }
}

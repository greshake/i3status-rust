#[cfg(feature = "pulseaudio")]
use std::cmp::min;
use std::cmp::max;
use std::io::Read;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
#[cfg(feature = "pulseaudio")]
use std::rc::Rc;
#[cfg(feature = "pulseaudio")]
use std::cell::RefCell;
#[cfg(feature = "pulseaudio")]
use std::sync::Mutex;
#[cfg(feature = "pulseaudio")]
use std::collections::HashMap;
#[cfg(feature = "pulseaudio")]
use std::ops::Deref;
use chan::Sender;
#[cfg(feature = "pulseaudio")]
use chan::{async, sync};

use scheduler::Task;
use block::{Block, ConfigBlock};
use config::Config;
use errors::*;
use widgets::button::ButtonWidget;
use widget::{I3BarWidget, State};
use input::{I3BarEvent, MouseButton};
use subprocess::{parse_command, spawn_child_async};

#[cfg(feature = "pulseaudio")]
use pulse::mainloop::standard::Mainloop;
#[cfg(feature = "pulseaudio")]
use pulse::callbacks::ListResult;
#[cfg(feature = "pulseaudio")]
use pulse::context::{Context, flags, State as PulseState, introspect::SinkInfo, introspect::ServerInfo, subscribe::Facility, subscribe::Operation as SubscribeOperation, subscribe::subscription_masks};
#[cfg(feature = "pulseaudio")]
use pulse::proplist::{properties, Proplist};
#[cfg(feature = "pulseaudio")]
use pulse::mainloop::standard::IterateResult;
#[cfg(feature = "pulseaudio")]
use pulse::volume::{ChannelVolumes, VOLUME_NORM, VOLUME_MAX};

use uuid::Uuid;

trait SoundDevice {
    fn volume(&self) -> u32;
    fn muted(&self) -> bool;

    fn get_info(&mut self) -> Result<()>;
    fn set_volume(&mut self, step: i32) -> Result<()>;
    fn toggle(&mut self) -> Result<()>;
    fn monitor(&mut self, id: String, tx_update_request: Sender<Task>) -> Result<()>;
}

struct AlsaSoundDevice {
    name: String,
    volume: u32,
    muted: bool,
}

impl AlsaSoundDevice {
    fn new(name: String) -> Result<Self> {
        let mut sd = AlsaSoundDevice {
            name,
            volume: 0,
            muted: false,
        };
        sd.get_info()?;

        Ok(sd)
    }
}

impl SoundDevice for AlsaSoundDevice {
    fn volume(&self) -> u32 { self.volume }
    fn muted(&self) -> bool { self.muted }

    fn get_info(&mut self) -> Result<()> {
        let output = Command::new("amixer")
            .args(&["get", &self.name])
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

        self.volume = last.get(0)
            .block_error("sound", "could not get volume")?
            .parse::<u32>()
            .block_error("sound", "could not parse volume to u32")?;

        self.muted = last.get(1).map(|muted| *muted == "off").unwrap_or(false);

        Ok(())
    }

    fn set_volume(&mut self, step: i32) -> Result<()> {
        let volume = max(0, self.volume as i32 + step) as u32;

        Command::new("amixer")
            .args(&[
                "set",
                &self.name,
                &format!("{}%", volume),
            ])
            .output()
            .block_error("sound", "failed to set volume")?;

        self.volume = volume;

        Ok(())
    }

    fn toggle(&mut self) -> Result<()> {
        Command::new("amixer")
            .args(&["set", &self.name, "toggle"])
            .output()
            .block_error("sound", "failed to toggle mute")?;

        self.muted = !self.muted;

        Ok(())
    }

    fn monitor(&mut self, id: String, tx_update_request: Sender<Task>) -> Result<()> {
        // Monitor volume changes in a separate thread.
        thread::spawn(move || {
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
                    tx_update_request.send(Task {
                        id: id.clone(),
                        update_time: Instant::now(),
                    });
                }
                // Don't update too often. Wait 1/4 second, fast enough for
                // volume button mashing but slow enough to skip event spam.
                thread::sleep(Duration::new(0, 250_000_000))
            }
        });

        Ok(())
    }
}

#[cfg(feature = "pulseaudio")]
struct PulseAudioConnection {
    mainloop: Rc<RefCell<Mainloop>>,
    context: Rc<RefCell<Context>>
}

#[cfg(feature = "pulseaudio")]
struct PulseAudioClient {
    sender: Sender<PulseAudioClientRequest>
}

#[cfg(feature = "pulseaudio")]
struct PulseAudioSoundDevice {
    name: Option<String>,
    volume: Option<ChannelVolumes>,
    volume_avg: u32,
    muted: bool,
}

#[cfg(feature = "pulseaudio")]
#[derive(Debug)]
struct PulseAudioSinkInfo {
    volume: ChannelVolumes,
    mute: bool,
}

#[cfg(feature = "pulseaudio")]
#[derive(Debug)]
enum PulseAudioClientRequest {
    GetDefaultDevice,
    GetSinkInfoByIndex(u32),
    GetSinkInfoByName(String),
    SetSinkVolumeByName(String, ChannelVolumes),
    SetSinkMuteByName(String, bool),
}

#[cfg(feature = "pulseaudio")]
lazy_static! {
    static ref PULSEAUDIO_CLIENT: Result<PulseAudioClient> = PulseAudioClient::new();
    static ref PULSEAUDIO_EVENT_LISTENER: Mutex<HashMap<String, Sender<Task>>> = Mutex::new(HashMap::new());
    static ref PULSEAUDIO_DEFAULT_SINK: Mutex<String> = Mutex::new("@DEFAULT_SINK@".into());
    static ref PULSEAUDIO_SINKS: Mutex<HashMap<String, PulseAudioSinkInfo>> = Mutex::new(HashMap::new());
}

#[cfg(feature = "pulseaudio")]
impl PulseAudioConnection {
    fn new() -> Result<Self> {
        let mut proplist = Proplist::new().unwrap();
        proplist.sets(properties::APPLICATION_NAME, "i3status-rs")
            .block_error("sound", "could not set pulseaudio APPLICATION_NAME poperty")?;

        let mainloop = Rc::new(RefCell::new(Mainloop::new()
            .block_error("sound", "failed to create pulseaudio mainloop")?));

        let context = Rc::new(RefCell::new(
            Context::new_with_proplist(
                mainloop.borrow().deref(),
                "i3status-rs_context",
                &proplist
            )
            .block_error("sound", "failed to create new pulseaudio context")?
        ));

        context.borrow_mut().connect(None, flags::NOFLAGS, None)
            .block_error("sound", "failed to connect to pulseaudio context")?;

        let mut connection = PulseAudioConnection {
            mainloop,
            context
        };

        // Wait for context to be ready
        loop {
            connection.iterate(false)?;
            match connection.context.borrow().get_state() {
                PulseState::Ready => { break; },
                PulseState::Failed |
                PulseState::Terminated => {
                    Err(BlockError(
                        "sound".into(),
                        "pulseaudio context state failed/terminated".into(),
                    )).unwrap()
                },
                _ => {},
            }
        }

        Ok(connection)
    }

    fn iterate(&mut self, blocking: bool) -> Result<()> {
        match self.mainloop.borrow_mut().iterate(blocking) {
            IterateResult::Quit(_) |
            IterateResult::Err(_) => {
                Err(BlockError(
                    "sound".into(),
                    "failed to iterate pulseaudio state".into(),
                ))
            },
            IterateResult::Success(_) => Ok(()),
        }
    }
}

#[cfg(feature = "pulseaudio")]
impl PulseAudioClient {
    fn new() -> Result<PulseAudioClient> {
        let (send_req, recv_req) = async();
        let (send_result, recv_result) = sync(0);
        let send_result2 = send_result.clone();
        let new_connection = |sender: Sender<Result<()>>| -> PulseAudioConnection {
            let conn = PulseAudioConnection::new();
            match conn {
                Ok(conn) => {
                    sender.send(Ok(()));
                    conn
                },
                Err(err) => {
                    sender.send(Err(err));
                    panic!("failed to create pulseaudio connection");
                }
            }
        };
        let thread_result = || -> Result<()> {
            match recv_result.recv() {
                None => {
                    Err(BlockError(
                        "sound".into(),
                        "failed to receive from pulseaudio thread channel".into()
                    ))
                },
                Some(result) => result
            }
        };

        // requests
        thread::spawn(move || {
            let mut connection = new_connection(send_result);

            loop {
                // make sure mainloop dispatched everything
                for _ in 0..10 {
                    connection.iterate(false).unwrap();
                }

                match recv_req.recv() {
                    None => { },
                    Some(req) => {
                        let mut introspector = connection.context.borrow_mut().introspect();

                        match req {
                            PulseAudioClientRequest::GetDefaultDevice => {
                                introspector.get_server_info(PulseAudioClient::server_info_callback);
                            },
                            PulseAudioClientRequest::GetSinkInfoByIndex(index) => {
                                introspector.get_sink_info_by_index(index, PulseAudioClient::sink_info_callback);
                            },
                            PulseAudioClientRequest::GetSinkInfoByName(name) => {
                                introspector.get_sink_info_by_name(&name, PulseAudioClient::sink_info_callback);
                            },
                            PulseAudioClientRequest::SetSinkVolumeByName(name, volumes) => {
                                introspector.set_sink_volume_by_name(&name, &volumes, None);
                            },
                            PulseAudioClientRequest::SetSinkMuteByName(name, mute) => {
                                introspector.set_sink_mute_by_name(&name, mute, None);
                            },
                        };

                        // send request and receive response
                        connection.iterate(true).unwrap();
                        connection.iterate(true).unwrap();
                    }
                }
            }
        });
        thread_result()?;

        // subscribe
        thread::spawn(move || {
            let connection = new_connection(send_result2);

            // subcribe for events
            connection.context.borrow_mut().set_subscribe_callback(Some(Box::new(PulseAudioClient::subscribe_callback)));
            connection.context.borrow_mut().subscribe(
                subscription_masks::SERVER |
                subscription_masks::SINK,
                |_| { }
            );

            connection.mainloop.borrow_mut().run().unwrap();
        });
        thread_result()?;

        Ok(PulseAudioClient{
            sender: send_req
        })
    }

    fn send(request: PulseAudioClientRequest) -> Result<()> {
        match PULSEAUDIO_CLIENT.as_ref() {
            Ok(client) => {
                client.sender.send(request);
                Ok(())
            },
            Err(err) => {
                Err(BlockError(
                    "sound".into(),
                    format!("pulseaudio connection failed with error: {}", err),
                ))
            }
        }
    }

    fn server_info_callback(server_info: &ServerInfo) {
        match server_info.default_sink_name.clone() {
            None => {},
            Some(default_sink) => {
                *PULSEAUDIO_DEFAULT_SINK.lock().unwrap() = default_sink.into();
                PulseAudioClient::send_update_event();
            }
        }
    }

    fn sink_info_callback(result: ListResult<&SinkInfo>) {
        match result {
            ListResult::End |
            ListResult::Error => { },
            ListResult::Item(sink_info) => {
                match sink_info.name.clone() {
                    None => {},
                    Some(name) => {
                        let info = PulseAudioSinkInfo {
                            volume: sink_info.volume,
                            mute: sink_info.mute,
                        };
                        PULSEAUDIO_SINKS.lock().unwrap().insert(name.into(), info);
                        PulseAudioClient::send_update_event();
                    }
                }
            },
        }
    }

    fn subscribe_callback(facility: Option<Facility>, _operation: Option<SubscribeOperation>, index: u32) {
        match facility {
            None => { },
            Some(facility) => match facility {
                Facility::Server => {
                    let _ = PulseAudioClient::send(PulseAudioClientRequest::GetDefaultDevice);
                },
                Facility::Sink => {
                    let _ = PulseAudioClient::send(PulseAudioClientRequest::GetSinkInfoByIndex(index));
                },
                _ => { }
            }
        }
    }

    fn send_update_event() {
        for (id, tx_update_request) in &*PULSEAUDIO_EVENT_LISTENER.lock().unwrap() {
            tx_update_request.send(Task {
                id: id.clone(),
                update_time: Instant::now(),
            });
        }
    }
}

#[cfg(feature = "pulseaudio")]
impl PulseAudioSoundDevice {
    fn new() -> Result<Self> {
        PulseAudioClient::send(PulseAudioClientRequest::GetDefaultDevice)?;

        let device = PulseAudioSoundDevice {
            name: None,
            volume: None,
            volume_avg: 0,
            muted: false,
        };

        PulseAudioClient::send(PulseAudioClientRequest::GetSinkInfoByName(device.name()))?;

        Ok(device)
    }

    fn with_name(name: String) -> Result<Self> {
        PulseAudioClient::send(PulseAudioClientRequest::GetSinkInfoByName(name.clone()))?;

        Ok(PulseAudioSoundDevice {
            name: Some(name),
            volume: None,
            volume_avg: 0,
            muted: false,
        })
    }

    fn name(&self) -> String {
        self.name.clone().unwrap_or_else(|| PULSEAUDIO_DEFAULT_SINK.lock().unwrap().clone())
    }

    fn volume(&mut self, volume: ChannelVolumes) {
        self.volume = Some(volume);
        self.volume_avg = (volume.avg().0 as f32 / VOLUME_NORM.0 as f32 * 100.0).round() as u32;
    }
}

#[cfg(feature = "pulseaudio")]
impl SoundDevice for PulseAudioSoundDevice {
    fn volume(&self) -> u32 { self.volume_avg }
    fn muted(&self) -> bool { self.muted }

    fn get_info(&mut self) -> Result<()> {
        match PULSEAUDIO_SINKS.lock().unwrap().get(&self.name()) {
            None => {},
            Some(sink_info) => {
                self.volume(sink_info.volume);
                self.muted = sink_info.mute;
            }
        }

        Ok(())
    }

    fn set_volume(&mut self, step: i32) -> Result<()> {
        let mut volume = match self.volume {
            Some(volume) => volume,
            None => return Err(BlockError("sound".into(), "volume unknown".into()))
        };

        // apply step to volumes
        let step = (step as f32 * VOLUME_NORM.0 as f32 / 100.0).round() as i32;
        for vol in volume.values.iter_mut() {
            vol.0 = min(max(0, vol.0 as i32 + step) as u32, VOLUME_MAX.0);
        }

        // update volumes
        self.volume(volume);
        PulseAudioClient::send(PulseAudioClientRequest::SetSinkVolumeByName(self.name(), volume))?;

        Ok(())
    }

    fn toggle(&mut self) -> Result<()> {
        self.muted = !self.muted;
        PulseAudioClient::send(PulseAudioClientRequest::SetSinkMuteByName(self.name(), self.muted))?;

        Ok(())
    }

    fn monitor(&mut self, id: String, tx_update_request: Sender<Task>) -> Result<()> {
        PULSEAUDIO_EVENT_LISTENER.lock().unwrap().insert(id, tx_update_request);
        Ok(())
    }
}

// TODO: Use the alsa control bindings to implement push updates
pub struct Sound {
    text: ButtonWidget,
    id: String,
    device: Box<SoundDevice>,
    step_width: u32,
    config: Config,
    on_click: Option<String>,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct SoundConfig {
    /// ALSA / PulseAudio sound device name
    #[serde(default = "SoundDriver::default")]
    pub driver: SoundDriver,

    /// ALSA / PulseAudio sound device name
    #[serde(default = "SoundConfig::default_name")]
    pub name: Option<String>,

    /// The steps volume is in/decreased for the selected audio device (When greater than 50 it gets limited to 50)
    #[serde(default = "SoundConfig::default_step_width")]
    pub step_width: u32,

    #[serde(default = "SoundConfig::default_on_click")]
    pub on_click: Option<String>,
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

    fn default_step_width() -> u32 {
        5
    }

    fn default_on_click() -> Option<String> {
        None
    }
}

impl Sound {
    fn display(&mut self) -> Result<()> {
        self.device.get_info()?;

        if self.device.muted() {
            self.text.set_icon("volume_empty");
            self.text.set_text(
                self.config
                    .icons
                    .get("volume_muted")
                    .block_error("sound", "cannot find icon")?
                    .to_owned(),
            );
            self.text.set_state(State::Warning);
        } else {
            let volume = self.device.volume();
            self.text.set_icon(match volume {
                0...20 => "volume_empty",
                21...70 => "volume_half",
                _ => "volume_full",
            });
            self.text.set_text(format!("{:02}%", volume));
            self.text.set_state(State::Idle);
        }

        Ok(())
    }
}

impl ConfigBlock for Sound {
    type Config = SoundConfig;

    fn new(block_config: Self::Config, config: Config, tx_update_request: Sender<Task>) -> Result<Self> {
        let id = Uuid::new_v4().simple().to_string();
        let mut step_width = block_config.step_width;
        if step_width > 50 {
            step_width = 50;
        }

        #[cfg(not(feature = "pulseaudio"))]
        type PulseAudioSoundDevice =  AlsaSoundDevice;

        // try to create a pulseaudio device if feature is enabled and `driver != "alsa"`
        let pulseaudio_device: Result<PulseAudioSoundDevice> = match block_config.driver {
            #[cfg(feature = "pulseaudio")]
            SoundDriver::Auto | SoundDriver::PulseAudio =>
                match block_config.name.clone() {
                    None => PulseAudioSoundDevice::new(),
                    Some(name) => PulseAudioSoundDevice::with_name(name)
                },
            _ => Err(BlockError(
                "sound".into(),
                "PulseAudio feature or driver disabled".into(),
            ))
        };

        // prefere PulseAudio if available and selected, fallback to ALSA
        let device: Box<SoundDevice> = match pulseaudio_device {
            Ok(dev) => Box::new(dev),
            Err(_) => Box::new(AlsaSoundDevice::new(block_config.name.unwrap_or_else(|| "Master".into()))?)
        };

        let mut sound = Self {
            text: ButtonWidget::new(config.clone(), &id).with_icon("volume_empty"),
            id: id.clone(),
            device,
            step_width,
            config,
            on_click: block_config.on_click,
        };

        sound.device.monitor(id.clone(), tx_update_request.clone())?;

        Ok(sound)
    }
}

// To filter [100%] output from amixer into 100
const FILTER: &[char] = &['[', ']', '%'];

impl Block for Sound {
    fn update(&mut self) -> Result<Option<Duration>> {
        self.display()?;
        Ok(None) // The monitor thread will call for updates when needed.
    }

    fn view(&self) -> Vec<&I3BarWidget> {
        vec![&self.text]
    }

    fn click(&mut self, e: &I3BarEvent) -> Result<()> {
        if let Some(ref name) = e.name {
            if name.as_str() == self.id {
                match e.button {
                    MouseButton::Right => self.device.toggle()?,
                    MouseButton::Left =>
                        if let Some(ref cmd) = self.on_click {
                            let (cmd_name, cmd_args) = parse_command(cmd);
                            spawn_child_async(cmd_name, &cmd_args)
                                .block_error("sound", "could not spawn child")?;
                        }
                    MouseButton::WheelUp => {
                        self.device.set_volume(self.step_width as i32)?;
                    }
                    MouseButton::WheelDown => {
                        self.device.set_volume(-(self.step_width as i32))?;
                    }
                    _ => ()
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

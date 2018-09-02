use std::cmp::{min, max};
use std::io::Read;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use std::ffi::OsStr;
#[cfg(feature = "pulseaudio")]
use std::rc::Rc;
#[cfg(feature = "pulseaudio")]
use std::cell::RefCell;
#[cfg(feature = "pulseaudio")]
use std::sync::{Arc, Mutex, Once, ONCE_INIT};
#[cfg(feature = "pulseaudio")]
use std::collections::HashMap;
use std::ops::Deref;
use chan::Sender;
#[cfg(feature = "pulseaudio")]
use chan::{async, sync, Receiver};

use scheduler::Task;
use block::{Block, ConfigBlock};
use config::Config;
use de::deserialize_duration;
use errors::*;
use widgets::button::ButtonWidget;
use widget::{I3BarWidget, State};
use input::{I3BarEvent, MouseButton};

#[cfg(feature = "pulseaudio")]
use pulse::mainloop::standard::Mainloop;
#[cfg(feature = "pulseaudio")]
use pulse::callbacks::ListResult;
#[cfg(feature = "pulseaudio")]
use pulse::context::{Context, flags, State as PulseState, introspect::SinkInfo, introspect::ServerInfo, subscribe::Facility, subscribe::Operation as SubscribeOperation};
#[cfg(feature = "pulseaudio")]
use pulse::proplist::{properties, Proplist};
#[cfg(feature = "pulseaudio")]
use pulse::mainloop::standard::IterateResult;
#[cfg(feature = "pulseaudio")]
use pulse::volume::{ChannelVolumes, VOLUME_NORM, VOLUME_MAX};
#[cfg(feature = "pulseaudio")]
use pulse::def::Retval;
#[cfg(feature = "pulseaudio")]
use pulse::operation::{Operation, State as OperationState};

use uuid::Uuid;

trait SoundDevice {
    // fn name(&self) -> String;
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
            name: name,
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
            .into_iter()
            .last()
            .block_error("sound", "could not get sound info")?;

        let last = last_line
            .split_whitespace()
            .into_iter()
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
                if let Ok(_) = monitor.read(&mut buffer) {
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
struct PulseAudioClient {
    sender: Sender<PulseAudioClientRequest>
}

#[cfg(feature = "pulseaudio")]
struct PulseAudioSoundDevice {
    client: PulseAudioClient,
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
    GetSinkInfoByName(String),
    // GetSinkInfoByIndex(u32),
    SetSinkVolumeByName(String, ChannelVolumes),
    // SetSinkVolumeByName(u32, ChannelVolumes),
    SetSinkMuteByName(String, bool),
    // SetSinkMuteByName(u32, bool),
    QuitClient,
}

lazy_static! {
    // static ref PULSEAUDIO_EVENT: (Sender<Task>, Receiver<Task>) = async();
    static ref PULSEAUDIO_EVENT_LISTENER: Mutex<HashMap<String, Sender<Task>>> = Mutex::new(HashMap::new());
    static ref PULSEAUDIO_DEFAULT_SINK: Mutex<String> = Mutex::new("@DEFAULT_SINK@".into());
    static ref PULSEAUDIO_SINKS: Mutex<HashMap<String, PulseAudioSinkInfo>> = Mutex::new(HashMap::new());
}

impl PulseAudioClient {
    fn connect() -> Result<(Rc<RefCell<Mainloop>>, Rc<RefCell<Context>>)> {
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

        // Wait for context to be ready
        loop {
            match mainloop.borrow_mut().iterate(false) {
                IterateResult::Quit(_) |
                IterateResult::Err(_) => {
                    Err(BlockError(
                        "sound".into(),
                        "failed to iterate pulseaudio state".into(),
                    )).unwrap()
                },
                IterateResult::Success(_) => { },
            };
            match context.borrow().get_state() {
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

        Ok((mainloop, context))
    }

    fn new() -> Result<PulseAudioClient> {
        let (send, recv) = async();

        thread::spawn(move || {
            let (mainloop, context) = PulseAudioClient::connect().unwrap();
            let mut introspector = context.borrow_mut().introspect();

            let iterate = |block| -> Result<()> {
                match mainloop.borrow_mut().iterate(block) {
                    IterateResult::Quit(_) |
                    IterateResult::Err(_) => {
                        Err(BlockError(
                            "sound".into(),
                            "failed to iterate pulseaudio state".into(),
                        ))
                    },
                    IterateResult::Success(_) => Ok(()),
                }
            };

            loop {
                match recv.recv() {
                    None => { },
                    Some(req) => {
                        println!("!!! req: {:?}", req);
                        match req {
                            PulseAudioClientRequest::GetDefaultDevice => {
                                introspector.get_server_info(PulseAudioClient::server_info_callback);
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
                            PulseAudioClientRequest::QuitClient => { break; }
                        };

                        iterate(false).unwrap();
                    }
                }
            }
        });

        PulseAudioClient::init_monitor();

        Ok(PulseAudioClient{
            sender: send
        })
    }

    fn init_monitor() {
        thread::spawn(move || {
            let (mainloop, context) = PulseAudioClient::connect().unwrap();

            let iterate = |block| -> Result<()> {
                match mainloop.borrow_mut().iterate(block) {
                    IterateResult::Quit(_) |
                    IterateResult::Err(_) => {
                        Err(BlockError(
                            "sound".into(),
                            "failed to iterate pulseaudio state".into(),
                        ))
                    },
                    IterateResult::Success(_) => Ok(()),
                }
            };
        
            // subcribe for events
            context.borrow_mut().set_subscribe_callback(Some(Box::new(PulseAudioClient::subscribe_callback)));
            iterate(false).unwrap();

            use pulse::context::subscribe::subscription_masks;
            context.borrow_mut().subscribe(
                subscription_masks::SERVER |
                subscription_masks::SINK,
                |_| { }
            );
            iterate(false).unwrap();

            loop {
                iterate(true).unwrap();
            }
        });
    }

    fn send(&self, request: PulseAudioClientRequest) {
        self.sender.send(request)
    }

    fn server_info_callback(server_info: &ServerInfo) {
        println!("!!! server_info_callback: {:?}", server_info);
        match server_info.default_sink_name.clone() {
            None => {},
            Some(default_sink) => {
                *PULSEAUDIO_DEFAULT_SINK.lock().unwrap() = default_sink.into();
                PulseAudioClient::send_update_event();
            }
        }
    }

    fn sink_info_callback<'r, 's>(result: ListResult<&'r SinkInfo>) {
        println!("!!! sink_info_callback");
        match result {
            ListResult::End |
            ListResult::Error => { },
            ListResult::Item(sink_info) => {
                println!("!!! {:?}", sink_info);
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

    fn subscribe_callback(facility: Option<Facility>, operation: Option<SubscribeOperation>, index: u32) {
        println!("!!! facility: {:?}, operation: {:?}, index: {:?}", facility, operation, index);
        // PulseAudioClient::send_update_event();

        /*
        match facility {
            None,
            Some(facility) => match facility {
                Facility::Server => {
                    // TODO: update default device 
                },
                Facility::Sink => {
                    // TODO: update sink info
                }
            }
        }

        facility.and_then(|f| match f {
            
        });
        */
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
        let client = PulseAudioClient::new()?;
        client.send(PulseAudioClientRequest::GetDefaultDevice);

        Ok(PulseAudioSoundDevice {
            client,
            name: None,
            volume: None,
            volume_avg: 0,
            muted: false,
        })
    }

    fn with_name(name: String) -> Result<Self> {
        let client = PulseAudioClient::new()?;
        client.send(PulseAudioClientRequest::GetSinkInfoByName(name.clone()));

        Ok(PulseAudioSoundDevice {
            client,
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
        // unimplemented!();

        // let sink_info = self.context.borrow().introspect().get_sink_info_by_name(&self.name(), PulseAudioSoundDevice::sink_callback);
        // self.wait_for(sink_info, "get_sink_info_by_name")?;

        // self.client.send(PulseAudioClientRequest::GetSinkInfoByName(self.name()));

        println!("!!! {:?}", *PULSEAUDIO_SINKS.lock().unwrap());
        // return Ok(());

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
        // let sink_update = self.context.borrow().introspect().set_sink_volume_by_name(&self.name(), &volume, None);
        // self.wait_for(sink_update, "set_sink_volume_by_name")?;
        // unimplemented!();
        self.client.send(PulseAudioClientRequest::SetSinkVolumeByName(self.name(), volume));

        Ok(())
    }

    fn toggle(&mut self) -> Result<()> {
        self.muted = !self.muted;
        // self.context.borrow().introspect().set_sink_mute_by_name(&self.name(), self.muted, None);
        self.client.send(PulseAudioClientRequest::SetSinkMuteByName(self.name(), self.muted));

        Ok(())
    }

    fn monitor(&mut self, id: String, tx_update_request: Sender<Task>) -> Result<()> {
        // TODO: listen to events

        /*
        let (send, recv) = sync(8);

        send.send(0);

        thread::spawn(move || {
            loop {
                recv.recv().unwrap();

                self.name = Some("Test".into());

                tx_update_request.send(Task {
                    id: id.clone(),
                    update_time: Instant::now(),
                });
            }
        });
        */

        // Ok(())

        PULSEAUDIO_EVENT_LISTENER.lock().unwrap().insert(id, tx_update_request);
        Ok(())

        // unimplemented!();
        /*
        self.context.borrow_mut().set_subscribe_callback(Some(Box::new(PulseAudioSoundDevice::subscribe_callback)));

        use pulse::context::subscribe::subscription_masks;

        let interest = subscription_masks::ALL |
            subscription_masks::SINK;

        let subscribe = self.context.borrow_mut().subscribe(
            interest,
            |_| { }
        );
        self.wait_for(subscribe, "subscribe")?;

        Ok(())
        */
    }
}

#[cfg(feature = "pulseaudio")]
impl Drop for PulseAudioSoundDevice {
    fn drop(&mut self) {
        self.client.send(PulseAudioClientRequest::QuitClient);
    }
}

// TODO: Use the alsa control bindings to implement push updates
pub struct Sound {
    text: ButtonWidget,
    id: String,
    update_interval: Duration,
    device: Box<SoundDevice>,
    step_width: u32,
    config: Config,
    on_click: Option<String>,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct SoundConfig {
    /// Update interval in seconds
    #[serde(default = "SoundConfig::default_interval", deserialize_with = "deserialize_duration")]
    pub interval: Duration,

    /// ALSA / PulseAudio sound device name
    #[serde(default = "SoundConfig::default_name")]
    pub name: Option<String>,

    /// The steps volume is in/decreased for the selected audio device (When greater than 50 it gets limited to 50)
    #[serde(default = "SoundConfig::default_step_width")]
    pub step_width: u32,

    #[serde(default = "SoundConfig::default_on_click")]
    pub on_click: Option<String>,
}

impl SoundConfig {
    fn default_interval() -> Duration {
        Duration::from_secs(2)
    }

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

        #[cfg(feature = "pulseaudio")]
        let pulseaudio_device = match block_config.name.clone() {
            None => PulseAudioSoundDevice::new(),
            Some(name) => PulseAudioSoundDevice::with_name(name)
        };
        #[cfg(not(feature = "pulseaudio"))]
        let pulseaudio_device = Err(BlockError(
            "sound".into(),
            "PulseAudio feature disabled".into(),
        ));
        
        let device: Box<SoundDevice> = match pulseaudio_device {
            Ok(dev) => Box::new(dev),
            Err(_) => Box::new(AlsaSoundDevice::new(block_config.name.unwrap_or_else(|| "Master".into()))?)
        };

        let mut sound = Self {
            text: ButtonWidget::new(config.clone(), &id).with_icon("volume_empty"),
            id: id.clone(),
            update_interval: block_config.interval,
            device,
            step_width: step_width,
            config: config,
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

        // TODO: fix monitor thread
        Ok(None) // The monitor thread will call for updates when needed.
        // Ok(Some(self.update_interval))
    }

    fn view(&self) -> Vec<&I3BarWidget> {
        vec![&self.text]
    }

    fn click(&mut self, e: &I3BarEvent) -> Result<()> {
        if let Some(ref name) = e.name {
            if name.as_str() == self.id {
                match e.button {
                    MouseButton::Right => self.device.toggle()?,
                    MouseButton::Left => {
                        let mut command = "".to_string();
                        if self.on_click.is_some() {
                            command = self.on_click.clone().unwrap();
                        }
                        if self.on_click.is_some() {
                            let command_broken: Vec<&str> = command.split_whitespace().collect();
                            let mut itr = command_broken.iter();
                            let mut _cmd = Command::new(OsStr::new(&itr.next().unwrap()))
                                .args(itr)
                                .spawn();
                        }
                    }
                    MouseButton::WheelUp => {
                        let step = self.step_width as i32;
                        self.device.set_volume(step)?;
                    }
                    MouseButton::WheelDown => {
                        let step = -(self.step_width as i32);
                        self.device.set_volume(step)?;
                    }
                    _ => {}
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

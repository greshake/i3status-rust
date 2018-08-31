use std::cmp::min;
use std::io::Read;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use std::ffi::OsStr;
use std::rc::Rc;
use std::cell::RefCell;
use std::ops::Deref;
use chan::Sender;

use scheduler::Task;
use block::{Block, ConfigBlock};
use config::Config;
use de::deserialize_duration;
use errors::*;
use widgets::button::ButtonWidget;
use widget::{I3BarWidget, State};
use input::{I3BarEvent, MouseButton};

use pulse::mainloop::standard::Mainloop;
use pulse::callbacks::ListResult;
use pulse::context::{Context, flags, State as PulseState};
use pulse::proplist::{properties, Proplist};
use pulse::mainloop::standard::IterateResult;
use pulse::volume::{ChannelVolumes, Volume};
use pulse::def::Retval;

use uuid::Uuid;

trait SoundDevice {
    // fn name(&self) -> String;
    fn volume(&self) -> u32;
    fn muted(&self) -> bool;

    fn get_info(&mut self) -> Result<()>;
    fn set_volume(&mut self, step: i32) -> Result<()>;
    fn toggle(&mut self) -> Result<()>;
    fn monitor(&mut self, id: String, tx_update_request: Sender<Task>) { }
}

enum GenericSoundDevice {
    AlsaSoundDevice(AlsaSoundDevice),
    PulseAudioSoundDevice(PulseAudioSoundDevice)
}

impl From<AlsaSoundDevice> for GenericSoundDevice {
    fn from(device: AlsaSoundDevice) -> Self {
        GenericSoundDevice::AlsaSoundDevice(device)
    }
}
impl From<PulseAudioSoundDevice> for GenericSoundDevice {
    fn from(device: PulseAudioSoundDevice) -> Self {
        GenericSoundDevice::PulseAudioSoundDevice(device)
    }
}


struct AlsaSoundDevice {
    name: String,
    volume: u32,
    muted: bool,
}

impl AlsaSoundDevice {
    fn new(name: &str) -> Result<Self> {
        let mut sd = AlsaSoundDevice {
            name: String::from(name),
            volume: 0,
            muted: false,
        };
        sd.get_info()?;

        Ok(sd)
    }
}

impl SoundDevice for AlsaSoundDevice {
    // fn name(&self) -> String { self.name }
    fn volume(&self) -> u32 { self.volume }
    fn muted(&self) -> bool { self.muted }

    fn get_info(&mut self) -> Result<()> {
        let output = Command::new("sh")
            .args(&["-c", format!("amixer get {}", self.name).as_str()])
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
        Command::new("sh")
            .args(&[
                "-c",
                format!(
                    "amixer set {} {}%",
                    self.name,
                    (self.volume as i32 + step) as u32
                ).as_str(),
            ])
            .output()
            .block_error("sound", "failed to set volume")?;

        self.volume = (self.volume as i32 + step) as u32;

        Ok(())
    }

    fn toggle(&mut self) -> Result<()> {
        Command::new("sh")
            .args(&["-c", format!("amixer set {} toggle", self.name).as_str()])
            .output()
            .block_error("sound", "failed to toggle mute")?;

        self.muted = !self.muted;

        Ok(())
    }

    fn monitor(&mut self, id: String, tx_update_request: Sender<Task>) {
        // Monitor volume changes in a separate thread.
        thread::spawn(move || {
            let mut monitor = Command::new("sh")
                .args(
                    &[
                        "-c",
                        // Line-buffer to reduce noise.
                        "stdbuf -oL alsactl monitor",
                    ],
                )
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
    }
}

struct PulseAudioSoundDevice {
    mainloop: Rc<RefCell<Mainloop>>,
    context: Rc<RefCell<Context>>,
    name: Option<String>,
    index: u32,
    volume: Option<ChannelVolumes>,
    volume_avg: u32,
    muted: bool,
}

impl PulseAudioSoundDevice {
    fn new(index: u32) -> Result<Self> {
        let mut proplist = Proplist::new().unwrap();
        proplist.sets(properties::APPLICATION_NAME, "i3status-rs")
            .block_error("sound", "could not set pulseaudio APPLICATION_NAME poperty")?;

        let mainloop = Rc::new(RefCell::new(Mainloop::new()
            .block_error("sound", "failed to create pulseaudio mainloop")?));

        let context = Rc::new(RefCell::new(Context::new_with_proplist(
            mainloop.borrow().deref(),
            "i3status-rs_context",
            &proplist
            ).block_error("sound", "failed to create new pulseaudio context")?));

        context.borrow_mut().connect(None, flags::NOFLAGS, None)
            .block_error("sound", "failed to connect to pulseaudio context")?;

        // Wait for context to be ready
        loop {
            match mainloop.borrow_mut().iterate(false) {
                IterateResult::Quit(_) |
                IterateResult::Err(_) => {
                    return Err(BlockError(
                        "sound".into(),
                        "failed to iterate pulseaudio state".into(),
                    ))
                },
                IterateResult::Success(_) => {},
            }
            match context.borrow().get_state() {
                PulseState::Ready => { break; },
                PulseState::Failed |
                PulseState::Terminated => {
                    return Err(BlockError(
                        "sound".into(),
                        "pulseaudio context state failed/terminated".into(),
                    ))
                },
                _ => {},
            }
        }

        let mut sd = PulseAudioSoundDevice {
            mainloop,
            context,
            name: None,
            index: 0,
            volume: None,
            volume_avg: 0,
            muted: false,
        };
        sd.get_info()?;

        Ok(sd)
    }

    fn volume(&mut self, volume: ChannelVolumes) {
        self.volume = Some(volume);
        self.volume_avg = volume.avg().0;
    }
}

impl SoundDevice for PulseAudioSoundDevice {
    // fn name(&self) -> String { self.name.unwrap_or_else(|| format!("#{}", self.index)) }
    fn volume(&self) -> u32 { self.volume_avg }
    fn muted(&self) -> bool { self.muted }

    fn get_info(&mut self) -> Result<()> {
        // TODO: Figure out how to get the callback working
        /*
        let sink_info = self.context.borrow().introspect().get_sink_info_by_index(self.index, |result|{
            match result {
                ListResult::End => {},
                ListResult::Item(sink_info) => {
                    self.name = sink_info.name.and_then(|v| Some(v.into()));
                    self.muted = sink_info.mute;
                    self.volume(sink_info.volume);
                },
                ListResult::Error => {
                    // TODO: Error handling
                }
            }
        });
        */

        Ok(())
    }

    fn set_volume(&mut self, step: i32) -> Result<()> {
        let mut volume = match self.volume {
            Some(volume) => volume,
            None => return Err(BlockError("sound".into(),"volume unknown".into()))
        };
        let val = Volume { 0: step.abs() as u32 };

        let volume = if step > 0 {
            volume.increase(val)
        } else if step < 0 {
            volume.decrease(val)
        } else {
            return Ok(());
        };

        match volume {
            Some(volume) => {
                self.volume(*volume);
                self.context.borrow().introspect().set_sink_volume_by_index(self.index, &volume, None);
                Ok(())
            },
            None => Err(BlockError("sound".into(), "failed to increase/decrease volume".into()))
        }
    }

    fn toggle(&mut self) -> Result<()> {
        self.muted = !self.muted;
        self.context.borrow().introspect().set_sink_mute_by_index(self.index, self.muted, None);

        Ok(())
    }

    fn monitor(&mut self, id: String, tx_update_request: Sender<Task>) {
        // TODO: listen to events
    }
}

impl Drop for PulseAudioSoundDevice {
    fn drop(&mut self) {
        self.mainloop.borrow_mut().quit(Retval(0));
    }
}

// TODO: Use the alsa control bindings to implement push updates
// TODO: Allow for custom audio devices instead of Master
pub struct Sound {
    text: ButtonWidget,
    id: String,
    devices: Vec<::std::result::Result<PulseAudioSoundDevice, AlsaSoundDevice>>,
    step_width: u32,
    current_idx: usize,
    config: Config,
    on_click: Option<String>,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct SoundConfig {
    /// Update interval in seconds
    #[serde(default = "SoundConfig::default_interval", deserialize_with = "deserialize_duration")]
    pub interval: Duration,

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

    fn default_step_width() -> u32 {
        5
    }

    fn default_on_click() -> Option<String> {
        None
    }
}

impl Sound {
    fn device(&mut self) -> Result<&mut SoundDevice> {
        match self.devices
            .get_mut(self.current_idx)
            .block_error("sound", "failed to get device")? {
            Ok(dev) => Ok(dev),
            Err(dev) => Ok(dev)
        }
    }

    fn display(&mut self) -> Result<()> {
        self.device()?.get_info()?;

        if self.device()?.muted() {
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
            let volume = self.device()?.volume();
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

        let mut sound = Sound {
            text: ButtonWidget::new(config.clone(), &id).with_icon("volume_empty"),
            id: id.clone(),
            devices: vec![
                // TODO: find better solution for mixed types
                match PulseAudioSoundDevice::new(0) {
                    Ok(dev) => Ok(dev),
                    Err(_) => Err(AlsaSoundDevice::new("Master")?)
                }
            ],
            step_width: step_width,
            current_idx: 0,
            config: config,
            on_click: block_config.on_click,
        };

        sound.devices.iter_mut().map(|dev|
            match dev {
                Ok(dev) => dev.monitor(id.clone(), tx_update_request.clone()),
                Err(dev) => dev.monitor(id.clone(), tx_update_request.clone())
            }
        );

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
                {
                    // Additional scope to not keep mutably borrowed device for too long
                    let volume = self.device()?.volume();

                    match e.button {
                        MouseButton::Right => self.device()?.toggle()?,
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
                            if volume < 100 {
                                let step = min(self.step_width, 100 - volume) as i32;
                                self.device()?.set_volume(step)?;
                            }
                        }
                        MouseButton::WheelDown => {
                            if volume >= self.step_width {
                                let step = -(self.step_width as i32);
                                self.device()?.set_volume(step)?;
                            }
                        }
                        _ => {}
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

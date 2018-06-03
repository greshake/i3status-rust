use std::cmp::min;
use std::io::Read;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use std::ffi::OsStr;
use chan::Sender;

use scheduler::Task;
use block::{Block, ConfigBlock};
use config::Config;
use de::deserialize_duration;
use errors::*;
use widgets::button::ButtonWidget;
use widget::{I3BarWidget, State};
use input::{I3BarEvent, MouseButton};

use uuid::Uuid;

struct SoundDevice {
    name: String,
    volume: u32,
    muted: bool,
}

impl SoundDevice {
    fn new(name: &str) -> Result<Self> {
        let mut sd = SoundDevice {
            name: String::from(name),
            volume: 0,
            muted: false,
        };
        sd.get_info()?;

        Ok(sd)
    }

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
}

// TODO: Use the alsa control bindings to implement push updates
// TODO: Allow for custom audio devices instead of Master
pub struct Sound {
    text: ButtonWidget,
    id: String,
    devices: Vec<SoundDevice>,
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
    fn display(&mut self) -> Result<()> {
        let device = self.devices
            .get_mut(self.current_idx)
            .block_error("sound", "failed to get device")?;
        device.get_info()?;

        if device.muted {
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
            self.text.set_icon(match device.volume {
                0...20 => "volume_empty",
                21...70 => "volume_half",
                _ => "volume_full",
            });
            self.text.set_text(format!("{:02}%", device.volume));
            self.text.set_state(State::Idle);
        }

        Ok(())
    }
}

impl ConfigBlock for Sound {
    type Config = SoundConfig;

    fn new(block_config: Self::Config, config: Config, tx_update_request: Sender<Task>) -> Result<Self> {
        let id = format!("{}", Uuid::new_v4().to_simple());
        let mut step_width = block_config.step_width;
        if step_width > 50 {
            step_width = 50;
        }

        let sound = Sound {
            text: ButtonWidget::new(config.clone(), &id).with_icon("volume_empty"),
            id: id.clone(),
            devices: vec![SoundDevice::new("Master")?],
            step_width: step_width,
            current_idx: 0,
            config: config,
            on_click: block_config.on_click,
        };

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
                    let device = self.devices
                        .get_mut(self.current_idx)
                        .block_error("sound", "failed to get device")?;
                    let volume = device.volume;

                    match e.button {
                        MouseButton::Right => device.toggle()?,
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
                                device.set_volume(
                                    min(self.step_width, 100 - volume) as i32,
                                )?;
                            }
                        }
                        MouseButton::WheelDown => {
                            if volume >= self.step_width {
                                device.set_volume(-(self.step_width as i32))?;
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

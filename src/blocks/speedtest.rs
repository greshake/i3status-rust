use std::time::{Duration, Instant};
use std::process::Command;
use std::thread::spawn;
use std::sync::{Arc, Mutex};
use chan::{async, Receiver, Sender};
use scheduler::Task;

use block::{Block, ConfigBlock};
use config::Config;
use de::deserialize_duration;
use errors::*;
use widgets::button::ButtonWidget;
use widget::{I3BarWidget, State};
use input::{I3BarEvent, MouseButton};

use uuid::Uuid;

pub struct SpeedTest {
    vals: Arc<Mutex<(bool, Vec<f32>)>>,
    text: Vec<ButtonWidget>,
    id: String,
    config: SpeedTestConfig,
    send: Sender<()>,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct SpeedTestConfig {
    /// Update interval in seconds
    #[serde(default = "SpeedTestConfig::default_interval", deserialize_with = "deserialize_duration")]
    pub interval: Duration,

    /// Mode of speed display, true => MB/s, false => Mb/s
    #[serde(default = "SpeedTestConfig::default_bytes")]
    pub bytes: bool,
}

impl SpeedTestConfig {
    fn default_interval() -> Duration {
        Duration::from_secs(1800)
    }

    fn default_bytes() -> bool {
        false
    }
}

fn get_values(bytes: bool) -> Result<String> {
    let mut cmd = Command::new("speedtest-cli");
    cmd.arg("--simple");
    if bytes {
        cmd.arg("--bytes");
    }
    String::from_utf8(
        cmd.output()
            .block_error("speedtest", "could not get speedtest-cli output")?
            .stdout,
    ).block_error("speedtest", "could not parse speedtest-cli output")
}

fn parse_values(output: String) -> Result<Vec<f32>> {
    let mut vals: Vec<f32> = Vec::with_capacity(3);

    for line in output.lines() {
        let mut word = line.split_whitespace();
        word.next();
        vals.push(word.next()
            .block_error("speedtest", "missing data")?
            .parse::<f32>()
            .block_error("speedtest", "Unable to parse data")?);
    }

    Ok(vals)
}

fn make_thread(recv: Receiver<()>, done: Sender<Task>, values: Arc<Mutex<(bool, Vec<f32>)>>, config: SpeedTestConfig, id: String) {
    spawn(move || loop {
        if let Some(_) = recv.recv() {
            if let Ok(output) = get_values(config.bytes) {
                if let Ok(vals) = parse_values(output) {
                    if vals.len() == 3 {
                        let (ref mut update, ref mut values) = *values
                            .lock()
                            .expect("main thread paniced while holding speedtest-values mutex");
                        *values = vals;

                        *update = true;

                        done.send(Task {
                            id: id.clone(),
                            update_time: Instant::now(),
                        })
                    }
                }
            }
        }
    });
}

impl ConfigBlock for SpeedTest {
    type Config = SpeedTestConfig;

    fn new(block_config: Self::Config, config: Config, done: Sender<Task>) -> Result<Self> {
        // Create all the things we are going to send and take for ourselves.
        let (send, recv): (Sender<()>, Receiver<()>) = async();
        let vals = Arc::new(Mutex::new((false, vec![])));
        let id = format!("{}", Uuid::new_v4().to_simple());

        // Make the update thread
        make_thread(recv, done, vals.clone(), block_config.clone(), id.clone());

        let ty = if block_config.bytes { "MB/s" } else { "Mb/s" };
        Ok(SpeedTest {
            vals,
            text: vec![
                ButtonWidget::new(config.clone(), &id)
                    .with_icon("ping")
                    .with_text("0ms"),
                ButtonWidget::new(config.clone(), &id)
                    .with_icon("net_down")
                    .with_text(&format!("0{}", ty)),
                ButtonWidget::new(config.clone(), &id)
                    .with_icon("net_up")
                    .with_text(&format!("0{}", ty)),
            ],
            id,
            send,
            config: block_config,
        })
    }
}

impl Block for SpeedTest {
    fn update(&mut self) -> Result<Option<Duration>> {
        let (ref mut updated, ref vals) = *self.vals
            .lock()
            .block_error("speedtest", "mutext poisoned")?;

        if *updated {
            *updated = false;

            if vals.len() == 3 {
                let ty = if self.config.bytes { "MB/s" } else { "Mb/s" };

                self.text[0].set_text(format!("{}ms", vals[0]));
                self.text[1].set_text(format!("{}{}", vals[1], ty));
                self.text[2].set_text(format!("{}{}", vals[2], ty));

                self.text[0].set_state(match_range!(vals[0], default: (State::Critical) {
                            0.0 ; 25.0 => State::Good,
                            25.0 ; 60.0 => State::Info,
                            60.0 ; 100.0 => State::Warning
                }));
            }

            Ok(None)
        } else {
            self.send.send(());
            Ok(Some(self.config.interval))
        }
    }

    fn click(&mut self, e: &I3BarEvent) -> Result<()> {
        if let Some(ref name) = e.name {
            if name.as_str() == self.id && e.button == MouseButton::Left {
                self.send.send(());
            }
        }
        Ok(())
    }

    fn view(&self) -> Vec<&I3BarWidget> {
        let mut new: Vec<&I3BarWidget> = Vec::with_capacity(self.text.len());
        for w in &self.text {
            new.push(w);
        }
        new
    }

    fn id(&self) -> &str {
        &self.id
    }
}

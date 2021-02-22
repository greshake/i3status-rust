use std::fmt;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::{unbounded, Receiver, Sender};
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::SharedConfig;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::input::{I3BarEvent, MouseButton};
use crate::scheduler::Task;
use crate::util::format_number;
use crate::widgets::text::TextWidget;
use crate::widgets::{I3BarWidget, State};

pub struct SpeedTest {
    id: usize,
    vals: Arc<Mutex<(bool, Vec<f32>)>>,
    text: Vec<TextWidget>,
    config: SpeedTestConfig,
    send: Sender<()>,
}

#[derive(Copy, Clone, Debug, Deserialize)]
pub enum Unit {
    B,
    K,
    M,
    G,
    T,
}

impl Default for Unit {
    fn default() -> Self {
        Unit::K
    }
}

impl fmt::Display for Unit {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct SpeedTestConfig {
    /// Update interval in seconds
    #[serde(
        default = "SpeedTestConfig::default_interval",
        deserialize_with = "deserialize_duration"
    )]
    pub interval: Duration,

    /// Mode of speed display, true => MB/s, false => Mb/s
    #[serde(default = "SpeedTestConfig::default_bytes")]
    pub bytes: bool,

    /// Number of digits to show for throughput indiciators.
    #[serde(default = "SpeedTestConfig::default_speed_digits")]
    pub speed_digits: usize,

    /// Minimum unit to display for throughput indicators.
    #[serde(default = "SpeedTestConfig::default_speed_min_unit")]
    pub speed_min_unit: Unit,
}

impl SpeedTestConfig {
    fn default_interval() -> Duration {
        Duration::from_secs(1800)
    }

    fn default_bytes() -> bool {
        false
    }

    fn default_speed_min_unit() -> Unit {
        Unit::M
    }

    fn default_speed_digits() -> usize {
        3
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
    )
    .block_error("speedtest", "could not parse speedtest-cli output")
}

fn parse_values(output: &str) -> Result<Vec<f32>> {
    let mut vals: Vec<f32> = Vec::with_capacity(3);

    for line in output.lines() {
        let mut word = line.split_whitespace();
        word.next();
        vals.push(
            word.next()
                .block_error("speedtest", "missing data")?
                .parse::<f32>()
                .block_error("speedtest", "Unable to parse data")?,
        );
    }

    Ok(vals)
}

fn make_thread(
    recv: Receiver<()>,
    done: Sender<Task>,
    values: Arc<Mutex<(bool, Vec<f32>)>>,
    config: SpeedTestConfig,
    id: usize,
) {
    thread::Builder::new()
        .name("speedtest".into())
        .spawn(move || loop {
            if recv.recv().is_ok() {
                if let Ok(output) = get_values(config.bytes) {
                    if let Ok(vals) = parse_values(&output) {
                        if vals.len() == 3 {
                            let (ref mut update, ref mut values) = *values
                                .lock()
                                .expect("main thread paniced while holding speedtest-values mutex");
                            *values = vals;

                            *update = true;

                            done.send(Task {
                                id,
                                update_time: Instant::now(),
                            })
                            .unwrap();
                        }
                    }
                }
            }
        })
        .unwrap();
}

impl ConfigBlock for SpeedTest {
    type Config = SpeedTestConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        shared_config: SharedConfig,
        done: Sender<Task>,
    ) -> Result<Self> {
        // Create all the things we are going to send and take for ourselves.
        let (send, recv): (Sender<()>, Receiver<()>) = unbounded();
        let vals = Arc::new(Mutex::new((false, vec![])));

        // Make the update thread
        make_thread(recv, done, vals.clone(), block_config.clone(), id);

        let ty = if block_config.bytes { "MB/s" } else { "Mb/s" };
        Ok(SpeedTest {
            vals,
            text: vec![
                TextWidget::new(id, 0, shared_config.clone())
                    .with_icon("ping")
                    .with_text("0ms"),
                TextWidget::new(id, 1, shared_config.clone())
                    .with_icon("net_down")
                    .with_text(&format!("0{}", ty)),
                TextWidget::new(id, 2, shared_config)
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
    fn update(&mut self) -> Result<Option<Update>> {
        let (ref mut updated, ref vals) = *self
            .vals
            .lock()
            .block_error("speedtest", "mutext poisoned")?;

        if *updated {
            *updated = false;

            if vals.len() == 3 {
                let ping = vals[0] as f64 / 1_000.0;
                let down = vals[1] as f64 * 1_000_000.0;
                let up = vals[2] as f64 * 1_000_000.0;
                self.text[0].set_text(format_number(ping, self.config.speed_digits, "", "s"));
                self.text[1].set_text(format_number(
                    down,
                    self.config.speed_digits,
                    &self.config.speed_min_unit.to_string(),
                    if self.config.bytes { "B/s" } else { "b/s" },
                ));
                self.text[2].set_text(format_number(
                    up,
                    self.config.speed_digits,
                    &self.config.speed_min_unit.to_string(),
                    if self.config.bytes { "B/s" } else { "b/s" },
                ));

                // ping is in seconds
                self.text[0].set_state(match (ping * 1000.) as i32 {
                    0..=25 => State::Good,
                    26..=60 => State::Info,
                    61..=100 => State::Warning,
                    _ => State::Critical,
                });
            }

            Ok(None)
        } else {
            self.send.send(())?;
            Ok(Some(self.config.interval.into()))
        }
    }

    fn click(&mut self, e: &I3BarEvent) -> Result<()> {
        if let MouseButton::Left = e.button {
            self.send.send(())?;
        }
        Ok(())
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        let mut new: Vec<&dyn I3BarWidget> = Vec::with_capacity(self.text.len());
        for w in &self.text {
            new.push(w);
        }
        new
    }

    fn id(&self) -> usize {
        self.id
    }
}

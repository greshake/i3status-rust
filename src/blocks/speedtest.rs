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
use crate::formatting::value::Value;
use crate::formatting::{FormatTemplate, RenderedWidget};
use crate::protocol::i3bar_event::{I3BarEvent, MouseButton};
use crate::scheduler::Task;
use crate::widgets::text::TextWidget;
use crate::widgets::{I3BarWidget, State};

pub struct SpeedTest {
    id: usize,
    vals: Arc<Mutex<(bool, Vec<f32>)>>,
    widgets: Vec<TextWidget>,
    format: FormatTemplate,
    interval: Duration,
    ping_icon: String,
    down_icon: String,
    up_icon: String,
    shared_config: SharedConfig,
    send: Sender<()>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct SpeedTestConfig {
    /// Format override
    pub format: String,

    /// Update interval in seconds
    #[serde(deserialize_with = "deserialize_duration")]
    pub interval: Duration,
}

impl Default for SpeedTestConfig {
    fn default() -> Self {
        Self {
            format: "{ping}{speed_down}{speed_up}".to_string(),
            interval: Duration::from_secs(1800),
        }
    }
}

fn get_values() -> Result<String> {
    let mut cmd = Command::new("speedtest-cli");
    cmd.arg("--simple");
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
    id: usize,
) {
    thread::Builder::new()
        .name("speedtest".into())
        .spawn(move || loop {
            if recv.recv().is_ok() {
                if let Ok(output) = get_values() {
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
        make_thread(recv, done, vals.clone(), id);

        Ok(SpeedTest {
            id,
            vals,
            widgets: Vec::new(),
            format: FormatTemplate::from_string(&block_config.format)?,
            interval: block_config.interval,
            ping_icon: shared_config.get_icon("ping")?,
            down_icon: shared_config.get_icon("net_down")?,
            up_icon: shared_config.get_icon("net_up")?,
            shared_config,
            send,
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
                // ping is in seconds
                let ping = vals[0] as f64 / 1_000.0;
                let down = vals[1] as f64 * 1_000_000.0;
                let up = vals[2] as f64 * 1_000_000.0;

                let values = map!(
                    "ping" => Value::from_float(ping).seconds().icon(self.ping_icon.clone()),
                    "speed_down" => Value::from_float(down).bits().icon(self.down_icon.clone()),
                    "speed_up" => Value::from_float(up).bits().icon(self.up_icon.clone()),
                );

                let ping_state = match (ping * 1000.) as i32 {
                    0..=25 => State::Good,
                    26..=60 => State::Info,
                    61..=100 => State::Warning,
                    _ => State::Critical,
                };

                let widgets =
                    self.format
                        .render_widgets(self.shared_config.clone(), self.id, &values)?;
                self.widgets.clear();
                for widget in widgets {
                    self.widgets.push(match widget {
                        RenderedWidget::Text(widget) => widget,
                        RenderedWidget::Var(name, widget) => match name.as_str() {
                            "ping" => widget.with_state(ping_state),
                            _ => widget,
                        },
                    });
                }
            }

            Ok(None)
        } else {
            self.send.send(())?;
            Ok(Some(self.interval.into()))
        }
    }

    fn click(&mut self, e: &I3BarEvent) -> Result<()> {
        if let MouseButton::Left = e.button {
            self.send.send(())?;
        }
        Ok(())
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        let mut retval = Vec::new();
        for widget in &self.widgets {
            retval.push(widget as &dyn I3BarWidget);
        }
        retval
    }

    fn id(&self) -> usize {
        self.id
    }
}

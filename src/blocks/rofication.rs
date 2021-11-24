use std::str::FromStr;
use std::time::Duration;

use crossbeam_channel::Sender;
use serde_derive::Deserialize;

use std::io::Read;
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::Path;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::SharedConfig;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::protocol::i3bar_event::I3BarEvent;
use crate::protocol::i3bar_event::MouseButton;
use crate::scheduler::Task;
use crate::subprocess::spawn_child_async;
use crate::widgets::text::TextWidget;
use crate::widgets::I3BarWidget;
use crate::widgets::State;

#[derive(Debug)]
struct RotificationStatus {
    num: u64,
    crit: u64,
}

pub struct Rofication {
    id: usize,
    text: TextWidget,
    update_interval: Duration,

    //useful, but optional
    #[allow(dead_code)]
    shared_config: SharedConfig,
    #[allow(dead_code)]
    tx_update_request: Sender<Task>,
    // UNIX socket to read from
    pub socket_path: String,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct RoficationConfig {
    /// Update interval in seconds
    #[serde(deserialize_with = "deserialize_duration")]
    pub interval: Duration,
    // UNIX socket to read from
    pub socket_path: String,
}

impl Default for RoficationConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(1),
            socket_path: String::from_str("/tmp/rofi_notification_daemon").unwrap(),
        }
    }
}

impl ConfigBlock for Rofication {
    type Config = RoficationConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        shared_config: SharedConfig,
        tx_update_request: Sender<Task>,
    ) -> Result<Self> {
        let text = TextWidget::new(id, 0, shared_config.clone())
            .with_text("0")
            .with_icon("bell")?
            .with_state(State::Good);

        Ok(Rofication {
            id,
            update_interval: block_config.interval,
            text,
            tx_update_request,
            shared_config,
            socket_path: block_config.socket_path,
        })
    }
}

impl Block for Rofication {
    fn update(&mut self) -> Result<Option<Update>> {
        match rofication_status(&self.socket_path) {
            Ok(status) => {
                self.text.set_icon("bell")?;
                self.text.set_text(status.num.to_string());
                if status.crit > 0 {
                    self.text.set_state(State::Critical)
                } else {
                    if status.num > 0 {
                        self.text.set_state(State::Warning)
                    } else {
                        self.text.set_state(State::Good)
                    }
                }
            }
            Err(_) => {
                self.text.set_text("?".to_string());
                self.text.set_state(State::Critical);
                self.text.set_icon("bell-slash")?;
            }
        }

        Ok(Some(self.update_interval.into()))
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.text]
    }

    fn click(&mut self, event: &I3BarEvent) -> Result<()> {
        if event.button == MouseButton::Left {
            spawn_child_async("rofication-gui", &[])
                .block_error("rofication", "could not spawn gui")?;
        }
        Ok(())
    }

    fn id(&self) -> usize {
        self.id
    }
}

fn rofication_status(socket_path: &str) -> Result<RotificationStatus> {
    let socket = Path::new(socket_path);
    // Connect to socket
    let mut stream = match UnixStream::connect(&socket) {
        Err(_) => {
            return Err(BlockError(
                "rofication".to_string(),
                "Failed to connect to socket".to_string(),
            ))
        }
        Ok(stream) => stream,
    };

    // Request count
    match stream.write(b"num\n") {
        Err(_) => {
            return Err(BlockError(
                "rofication".to_string(),
                "Failed to write to socket".to_string(),
            ))
        }
        Ok(_) => {}
    };

    // Response must be two comma separated integers: regular and critical
    let mut buffer = String::new();
    match stream.read_to_string(&mut buffer) {
        Err(_) => {
            return Err(BlockError(
                "rofication".to_string(),
                "Failed to read from socket".to_string(),
            ))
        }
        Ok(_) => {}
    };

    let values = buffer.split(',').collect::<Vec<&str>>();
    if values.len() != 2 {
        return Err(BlockError(
            "rofication".to_string(),
            "Format error".to_string(),
        ));
    }

    let num = match values[0].parse::<u64>() {
        Ok(num) => num,
        Err(_) => {
            return Err(BlockError(
                "rofication".to_string(),
                "Failed to parse num".to_string(),
            ))
        }
    };
    let crit = match values[1].parse::<u64>() {
        Ok(crit) => crit,
        Err(_) => {
            return Err(BlockError(
                "rofication".to_string(),
                "Failed to parse crit".to_string(),
            ))
        }
    };

    Ok(RotificationStatus { num, crit })
}

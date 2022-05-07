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
use crate::formatting::value::Value;
use crate::formatting::FormatTemplate;
use crate::protocol::i3bar_event::I3BarEvent;
use crate::protocol::i3bar_event::MouseButton;
use crate::scheduler::Task;
use crate::subprocess::spawn_child_async;
use crate::util::expand_string;
use crate::widgets::text::TextWidget;
use crate::widgets::I3BarWidget;
use crate::widgets::State;

#[derive(Debug)]
struct RotificationStatus {
    num: u64,
    crit: u64,
}

pub struct Rofication {
    text: TextWidget,
    update_interval: Duration,
    format: FormatTemplate,

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

    /// Format override
    pub format: FormatTemplate,
}

impl Default for RoficationConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(1),
            socket_path: "/tmp/rofi_notification_daemon".to_string(),
            format: FormatTemplate::default(),
        }
    }
}

impl ConfigBlock for Rofication {
    type Config = RoficationConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        shared_config: SharedConfig,
        _tx_update_request: Sender<Task>,
    ) -> Result<Self> {
        let text = TextWidget::new(id, 0, shared_config)
            .with_text("?")
            .with_icon("bell")?
            .with_state(State::Good);

        Ok(Rofication {
            update_interval: block_config.interval,
            text,
            socket_path: expand_string(&block_config.socket_path)?,
            format: block_config.format.with_default("{num}")?,
        })
    }
}

impl Block for Rofication {
    fn name(&self) -> &'static str {
        "rofication"
    }

    fn update(&mut self) -> Result<Option<Update>> {
        match rofication_status(&self.socket_path) {
            Ok(status) => {
                let values = map!(
                    "num" => Value::from_string(status.num.to_string()),
                );

                self.text.set_texts(self.format.render(&values)?);
                if status.crit > 0 {
                    self.text.set_state(State::Critical)
                } else if status.num > 0 {
                    self.text.set_state(State::Warning)
                } else {
                    self.text.set_state(State::Good)
                }
                self.text.set_icon("bell")?;
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
            spawn_child_async("rofication-gui", &[]).error_msg("could not spawn gui")?;
        }
        Ok(())
    }
}

fn rofication_status(socket_path: &str) -> Result<RotificationStatus> {
    let socket = Path::new(socket_path);
    // Connect to socket
    let mut stream = UnixStream::connect(&socket).error_msg("failed to connect to socket")?;

    // Request count
    stream
        .write(b"num:\n")
        .error_msg("Failed to write to socket")?;

    // Response must be two comma separated integers: regular and critical
    let mut buffer = String::new();
    stream
        .read_to_string(&mut buffer)
        .error_msg("Failed to read from socket")?;

    // Original rofication uses newline to separate, while regolith forks uses comma.
    let values = buffer
        .split_once(|c| c == ',' || c == '\n')
        .error_msg("Format error")?;

    let num = values.0.parse::<u64>().error_msg("Failed to parse num")?;
    let crit = values.1.parse::<u64>().error_msg("Failed to parse crit")?;

    Ok(RotificationStatus { num, crit })
}

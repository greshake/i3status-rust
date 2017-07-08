use std::time::Duration;
use std::sync::mpsc::Sender;

use block::{Block, ConfigBlock};
use config::Config;
use de::deserialize_duration;
use errors::*;
use widgets::text::TextWidget;
use widgets::graph::GraphWidget;
use widget::I3BarWidget;
use scheduler::Task;
use std::fs::OpenOptions;
use std::io::prelude::*;

use uuid::Uuid;

pub struct Net {
    output_rx: TextWidget,
    graph_rx: GraphWidget,
    output_tx: TextWidget,
    graph_tx: GraphWidget,
    id: String,
    update_interval: Duration,
    device_path: String,
    rx_buff: Vec<u64>,
    tx_buff: Vec<u64>,
    rx_bytes: u64,
    tx_bytes: u64,
    graph: bool,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct NetConfig {
    /// Update interval in seconds
    #[serde(default = "NetConfig::default_interval", deserialize_with = "deserialize_duration")]
    pub interval: Duration,

    /// Which interface in /sys/class/net/ to read from.
    //#[serde(default = "NetConfig::default_device")]
    pub device: String,
    pub graph: bool,
}

impl NetConfig {
    fn default_interval() -> Duration {
        Duration::from_secs(1)
    }
}

impl ConfigBlock for Net {
    type Config = NetConfig;

    fn new(block_config: Self::Config, config: Config, _tx_update_request: Sender<Task>) -> Result<Self> {
        Ok(Net {
            id: Uuid::new_v4().simple().to_string(),
            update_interval: block_config.interval,
            output_tx: TextWidget::new(config.clone()).with_icon("net_up"),
            graph_tx: GraphWidget::new(config.clone()),
            output_rx: TextWidget::new(config.clone()).with_icon("net_down"),
            graph_rx: GraphWidget::new(config.clone()),
            device_path: format!("/sys/class/net/{}/statistics/", block_config.device),
            rx_buff: vec![0,0,0,0,0,0,0,0,0,0],
            tx_buff: vec![0,0,0,0,0,0,0,0,0,0],
            rx_bytes: 0,
            tx_bytes: 0,
            graph: block_config.graph,
        })
    }
}

fn read_file(path: &str) -> Result<String> {
    let mut f = OpenOptions::new().read(true).open(path).block_error(
        "net",
        &format!("failed to open file {}", path),
    )?;
    let mut content = String::new();
    f.read_to_string(&mut content).block_error(
        "net",
        &format!("failed to read {}", path),
    )?;
    // Removes trailing newline
    content.pop();
    Ok(content)
}

fn convert_speed(speed: u64) -> (f64, &'static str) {
    // the values for the match are so the speed doesn't go above 3 characters
    let (speed, unit) = match speed {
        x if x > 1047527424 => {(speed as f64 / 1073741824.0, "G")},
        x if x > 1022976 => {(speed as f64 / 1048576.0, "M")},
        x if x > 999 => {(speed as f64 / 1024.0, "K")},
        _ => (speed as f64, "B"),
    };
    (speed, unit)
}

impl Block for Net {
    fn update(&mut self) -> Result<Option<Duration>> {
        let current_rx = read_file(&format!("{}rx_bytes", self.device_path))?
            .parse::<u64>()
            .block_error("net", "failed to parse rx_bytes")?;
        let rx_bytes = (current_rx - self.rx_bytes) / self.update_interval.as_secs();
        let (rx_speed, rx_unit) = convert_speed(rx_bytes);
        self.rx_bytes = current_rx;

        let current_tx = read_file(&format!("{}tx_bytes", self.device_path))?
            .parse::<u64>()
            .block_error("net", "failed to parse tx_bytes")?;
        let tx_bytes = (current_tx - self.tx_bytes) / self.update_interval.as_secs();
        let (tx_speed, tx_unit) = convert_speed(tx_bytes);
        self.tx_bytes = current_tx;

        if self.graph {
            self.rx_buff.remove(0);
            self.rx_buff.push(rx_bytes);

            self.tx_buff.remove(0);
            self.tx_buff.push(tx_bytes);

            self.graph_tx.set_values(&self.tx_buff, None, None);
            self.graph_rx.set_values(&self.rx_buff, None, None);
        }

        self.output_tx.set_text(format!("{:5.1}{}", tx_speed, tx_unit));
        self.output_rx.set_text(format!("{:5.1}{}", rx_speed, rx_unit));
        Ok(Some(self.update_interval))
    }

    fn view(&self) -> Vec<&I3BarWidget> {
        if self.graph {
            return vec![&self.output_tx, &self.graph_tx, &self.output_rx, &self.graph_rx]
        }
        vec![&self.output_tx, &self.output_rx]
    }

    fn id(&self) -> &str {
        &self.id
    }
}

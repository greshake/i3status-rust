use std::fs::OpenOptions;
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use std::time::Duration;
use chan::Sender;

use block::{Block, ConfigBlock};
use config::Config;
use de::deserialize_duration;
use errors::*;
use widgets::text::TextWidget;
use widgets::graph::GraphWidget;
use widget::I3BarWidget;
use scheduler::Task;

use uuid::Uuid;

pub struct NetworkDevice {
    device: String,
    device_path: PathBuf,
    wireless: bool,
}

impl NetworkDevice {
    /// Use the network device `device`. Raises an error if a directory for that
    /// device is not found.
    pub fn from_device(device: String) -> Result<Self> {
        let device_path = Path::new("/sys/class/net").join(device.clone());
        if !device_path.exists() {
            return Err(BlockError(
                "net".to_string(),
                format!(
                    "Network device '{}' does not exist",
                    device_path.to_string_lossy()
                ),
            ));
        }

        // I don't believe that this should ever change, so set it now:
        let wireless = device_path.join("wireless").exists();

        Ok(NetworkDevice {
            device: device,
            device_path: device_path,
            wireless: wireless,
        })
    }

    /// Query the device for the current `tx_bytes` statistic.
    pub fn tx_bytes(&self) -> Result<u64> {
        try!(read_file(&self.device_path.join("statistics/tx_bytes")))
            .parse::<u64>()
            .block_error("net", "Failed to parse tx_bytes")
    }

    /// Query the device for the current `rx_bytes` statistic.
    pub fn rx_bytes(&self) -> Result<u64> {
        try!(read_file(&self.device_path.join("statistics/rx_bytes")))
            .parse::<u64>()
            .block_error("net", "Failed to parse rx_bytes")
    }

    /// Checks whether this device is wireless.
    pub fn is_wireless(&self) -> bool {
        self.wireless
    }
}

pub struct Net {
    output_rx: TextWidget,
    graph_rx: GraphWidget,
    output_tx: TextWidget,
    graph_tx: GraphWidget,
    id: String,
    update_interval: Duration,
    device: NetworkDevice,
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
    #[serde(default = "NetConfig::default_device")]
    pub device: String,

    #[serde(default = "NetConfig::default_graph")]
    pub graph: bool,
}

impl NetConfig {
    fn default_interval() -> Duration {
        Duration::from_secs(1)
    }

    fn default_device() -> String {
        "lo".to_string()
    }

    fn default_graph() -> bool {
        false
    }
}

impl ConfigBlock for Net {
    type Config = NetConfig;

    fn new(block_config: Self::Config, config: Config, _tx_update_request: Sender<Task>) -> Result<Self> {
        let device = try!(NetworkDevice::from_device(block_config.device));
        Ok(Net {
            id: Uuid::new_v4().simple().to_string(),
            update_interval: block_config.interval,
            output_tx: TextWidget::new(config.clone()).with_icon("net_up"),
            graph_tx: GraphWidget::new(config.clone()),
            output_rx: TextWidget::new(config.clone()).with_icon("net_down"),
            graph_rx: GraphWidget::new(config.clone()),
            device: device,
            rx_buff: vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            tx_buff: vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            rx_bytes: 0,
            tx_bytes: 0,
            graph: block_config.graph,
        })
    }
}

fn read_file(path: &Path) -> Result<String> {
    let mut f = OpenOptions::new().read(true).open(path).block_error(
        "net",
        &format!(
            "failed to open file {}",
            path.to_string_lossy()
        ),
    )?;
    let mut content = String::new();
    f.read_to_string(&mut content).block_error(
        "net",
        &format!(
            "failed to read {}",
            path.to_string_lossy()
        ),
    )?;
    // Removes trailing newline
    content.pop();
    Ok(content)
}

fn convert_speed(speed: u64) -> (f64, &'static str) {
    // the values for the match are so the speed doesn't go above 3 characters
    let (speed, unit) = match speed {
        x if x > 999_999_999 => (speed as f64 / 1_000_000_000.0, "G"),
        x if x > 999_999 => (speed as f64 / 1_000_000.0, "M"),
        x if x > 999 => (speed as f64 / 1_000.0, "k"),
        _ => (speed as f64, "B"),
    };
    (speed, unit)
}

impl Block for Net {
    fn update(&mut self) -> Result<Option<Duration>> {
        let current_rx = self.device.rx_bytes()?;
        let update_interval = (self.update_interval.as_secs() as f64) + (self.update_interval.subsec_nanos() as f64 / 1_000_000_000.0);
        let rx_bytes = ((current_rx - self.rx_bytes) as f64 / update_interval) as u64;
        let (rx_speed, rx_unit) = convert_speed(rx_bytes);
        self.rx_bytes = current_rx;

        let current_tx = self.device.tx_bytes()?;
        let tx_bytes = ((current_tx - self.tx_bytes) as f64 / update_interval) as u64;
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

        self.output_tx
            .set_text(format!("{:5.1}{}", tx_speed, tx_unit));
        self.output_rx
            .set_text(format!("{:5.1}{}", rx_speed, rx_unit));
        Ok(Some(self.update_interval))
    }

    fn view(&self) -> Vec<&I3BarWidget> {
        if self.graph {
            return vec![
                &self.output_tx,
                &self.graph_tx,
                &self.output_rx,
                &self.graph_rx,
            ];
        }
        vec![&self.output_tx, &self.output_rx]
    }

    fn id(&self) -> &str {
        &self.id
    }
}

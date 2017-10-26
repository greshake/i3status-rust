use std::fs::OpenOptions;
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use std::process::Command;
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

    /// Check whether this network device is in the `up` state. Note that a
    /// device that is not `up` is not necessarily `down`.
    pub fn is_up(&self) -> Result<bool> {
        let operstate = try!(read_file(&self.device_path.join("operstate")));
        Ok(operstate == "up")
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

    /// Queries the wireless SSID of this device (using `iw`), if it is
    /// connected to one.
    pub fn ssid(&self) -> Result<Option<String>> {
        let up = try!(self.is_up());
        if !self.wireless || !up {
            return Err(BlockError(
                "net".to_string(),
                "SSIDs are only available for connected wireless devices."
                    .to_string(),
            ));
        }
        let mut iw_output = try!(
            Command::new("sh")
                .args(
                    &[
                        "-c",
                        &format!(
                            "iw dev {} link | grep \"^\\sSSID:\" | sed \"s/^\\sSSID:\\s//g\"",
                            self.device
                        ),
                    ],
                )
                .output()
                .block_error("net", "Failed to exectute SSID query.")
        ).stdout;

        if iw_output.len() == 0 {
            Ok(None)
        } else {
            iw_output.pop(); // Remove trailing newline.
            String::from_utf8(iw_output)
                .block_error("net", "Non-UTF8 SSID.")
                .map(|s| Some(s))
        }
    }

    /// Queries the inet IP of this device (using `ip`).
    pub fn ip_addr(&self) -> Result<Option<String>> {
        if !self.is_up()? {
            return Ok(None);
        }
        let mut ip_output = try!(
            Command::new("sh")
                .args(
                    &[
                        "-c",
                        &format!(
                            "ip -oneline -family inet address show {} | sed -rn \"s/.*inet ([\\.0-9/]+).*/\\1/p\"",
                            self.device
                        ),
                    ],
                )
                .output()
                .block_error("net", "Failed to exectute IP address query.")
        ).stdout;

        if ip_output.len() == 0 {
            Ok(None)
        } else {
            ip_output.pop(); // Remove trailing newline.
            String::from_utf8(ip_output)
                .block_error("net", "Non-UTF8 IP address.")
                .map(|s| Some(s))
        }
    }
}

pub struct Net {
    network: TextWidget,
    ip: TextWidget,
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
    show_down: bool,
    ssid: Option<String>,
    show_ssid: bool,
    ip_addr: Option<String>,
    show_ip: bool,
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

    /// Whether to show networks that are down.
    #[serde(default = "NetConfig::default_show_down")]
    pub show_down: bool,

    /// Whether to show the SSID of active wireless networks.
    #[serde(default = "NetConfig::default_show_ssid")]
    pub show_ssid: bool,

    /// Whether to show the IP address of active networks.
    #[serde(default = "NetConfig::default_show_ip")]
    pub show_ip: bool,
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

    fn default_show_down() -> bool {
        true
    }

    fn default_show_ssid() -> bool {
        false
    }

    fn default_show_ip() -> bool {
        false
    }
}

impl ConfigBlock for Net {
    type Config = NetConfig;

    fn new(block_config: Self::Config, config: Config, _tx_update_request: Sender<Task>) -> Result<Self> {
        let device = try!(NetworkDevice::from_device(block_config.device));
        let wireless = device.is_wireless();
        Ok(Net {
            id: Uuid::new_v4().simple().to_string(),
            update_interval: block_config.interval,
            network: TextWidget::new(config.clone()).with_icon(match wireless {
                true => "net_wireless",
                false => "net_wired",
            }),
            ip: TextWidget::new(config.clone()),
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
            show_down: block_config.show_down,
            ssid: None,
            show_ssid: block_config.show_ssid && wireless,
            ip_addr: None,
            show_ip: block_config.show_ip,
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
        // Skip displaying tx/rx if device is not up.
        let is_up = try!(self.device.is_up());
        if !is_up {
            // Remove any residual SSID and IP address.
            if self.ssid.is_some() {
                self.ssid = None;
            }
            if self.ip_addr.is_some() {
                self.ip_addr = None;
            }
            self.network.set_text("×".to_string());
            self.output_tx.set_text("×".to_string());
            self.output_rx.set_text("×".to_string());
            return Ok(Some(self.update_interval));
        }

        // Only retreive the SSID & IP address when the network status changes
        // from down to up, since this request is expensive.
        if self.ssid.is_none() && self.device.is_wireless() && self.show_ssid {
            let ssid = try!(self.device.ssid());
            if ssid.is_some() {
                self.network.set_text(ssid.clone().unwrap());
            }
            self.ssid = ssid;
        }
        if self.ip_addr.is_none() && self.show_ip {
            let ip_addr = try!(self.device.ip_addr());
            if ip_addr.is_some() {
                self.ip.set_text(ip_addr.clone().unwrap());
            }
            self.ip_addr = ip_addr;
        }

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

        self.output_tx.set_text(
            format!("{:5.1}{}", tx_speed, tx_unit),
        );
        self.output_rx.set_text(
            format!("{:5.1}{}", rx_speed, rx_unit),
        );
        Ok(Some(self.update_interval))
    }

    fn view(&self) -> Vec<&I3BarWidget> {
        // Since we can't error here, report errors as non-up.
        let is_up = match self.device.is_up() {
            Ok(status) => status,
            Err(_) => false,
        };
        if is_up {
            let mut widgets: Vec<&I3BarWidget> = Vec::with_capacity(6);
            widgets.push(&self.network);
            if self.show_ip {
                widgets.push(&self.ip);
            }
            if self.graph {
                widgets.append(&mut vec![
                    &self.output_tx,
                    &self.graph_tx,
                    &self.output_rx,
                    &self.graph_rx,
                ]);
            } else {
                widgets.append(&mut vec![&self.output_tx, &self.output_rx]);
            }
            widgets
        } else if self.show_down {
            vec![&self.network]
        } else {
            vec![]
        }
    }

    fn id(&self) -> &str {
        &self.id
    }
}

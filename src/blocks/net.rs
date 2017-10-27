use std::fs::OpenOptions;
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};
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
        let operstate_file = self.device_path.join("operstate");
        if !operstate_file.exists() {
            // It seems more reasonable to treat these as inactive networks as
            // opposed to erroring out the entire block.
            Ok(false)
        } else {
            let operstate = try!(read_file(&operstate_file));
            Ok(operstate == "up")
        }
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
    ssid: Option<TextWidget>,
    ip_addr: Option<TextWidget>,
    output_rx: TextWidget,
    graph_rx: Option<GraphWidget>,
    output_tx: TextWidget,
    graph_tx: Option<GraphWidget>,
    id: String,
    update_interval: Duration,
    device: NetworkDevice,
    rx_buff: Vec<u64>,
    tx_buff: Vec<u64>,
    rx_bytes: u64,
    tx_bytes: u64,
    hide_inactive: bool,
    last_update: Instant,
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

    /// Whether to show the SSID of active wireless networks.
    #[serde(default = "NetConfig::default_ssid")]
    pub ssid: bool,

    /// Whether to show the IP address of active networks.
    #[serde(default = "NetConfig::default_ip")]
    pub ip: bool,

    /// Whether to hide networks that are down/inactive completely.
    #[serde(default = "NetConfig::default_hide_inactive")]
    pub hide_inactive: bool,
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

    fn default_hide_inactive() -> bool {
        false
    }

    fn default_ssid() -> bool {
        false
    }

    fn default_ip() -> bool {
        false
    }
}

impl ConfigBlock for Net {
    type Config = NetConfig;

    fn new(block_config: Self::Config, config: Config, _tx_update_request: Sender<Task>) -> Result<Self> {
        let device = try!(NetworkDevice::from_device(block_config.device));
        let init_rx_bytes = try!(device.rx_bytes());
        let init_tx_bytes = try!(device.tx_bytes());
        let wireless = device.is_wireless();
        Ok(Net {
            id: Uuid::new_v4().simple().to_string(),
            update_interval: block_config.interval,
            network: TextWidget::new(config.clone()).with_icon(match wireless {
                true => "net_wireless",
                false => "net_wired",
            }),
            // Might want to signal an error if the user wants the SSID of a
            // wired connection instead.
            ssid: match block_config.ssid && wireless {
                true => Some(TextWidget::new(config.clone())),
                false => None,
            },
            ip_addr: match block_config.ip {
                true => Some(TextWidget::new(config.clone())),
                false => None,
            },
            output_tx: TextWidget::new(config.clone()).with_icon("net_up"),
            graph_tx: match block_config.graph {
                true => Some(GraphWidget::new(config.clone())),
                false => None,
            },
            output_rx: TextWidget::new(config.clone()).with_icon("net_down"),
            graph_rx: match block_config.graph {
                true => Some(GraphWidget::new(config.clone())),
                false => None,
            },
            device: device,
            rx_buff: vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            tx_buff: vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            rx_bytes: init_rx_bytes,
            tx_bytes: init_tx_bytes,
            hide_inactive: block_config.hide_inactive,
            last_update: Instant::now(),
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
        // Skip updating tx/rx if device is not up.
        let is_up = try!(self.device.is_up());
        if !is_up {
            self.network.set_text("×".to_string());
            self.output_tx.set_text("×".to_string());
            self.output_rx.set_text("×".to_string());

            return Ok(Some(self.update_interval));
        } else {
            self.network.set_text("".to_string());
        }

        // Update SSID and IP address every 30s.
        let now = Instant::now();
        if now.duration_since(self.last_update).as_secs() > 30 {
            if let Some(ref mut widget) = self.ssid {
                let ssid = try!(self.device.ssid());
                if ssid.is_some() {
                    widget.set_text(ssid.unwrap());
                }
            }
            if let Some(ref mut widget) = self.ip_addr {
                let ip_addr = try!(self.device.ip_addr());
                if ip_addr.is_some() {
                    widget.set_text(ip_addr.unwrap());
                }
            }
            self.last_update = now;
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

        // Update the graph widgets, if they are enabled.
        if let Some(ref mut widget) = self.graph_rx {
            self.rx_buff.remove(0);
            self.rx_buff.push(rx_bytes);
            widget.set_values(&self.rx_buff, None, None);
        }
        if let Some(ref mut widget) = self.graph_tx {
            self.tx_buff.remove(0);
            self.tx_buff.push(tx_bytes);
            widget.set_values(&self.tx_buff, None, None);
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
            let mut widgets: Vec<&I3BarWidget> = Vec::with_capacity(7);
            widgets.push(&self.network);
            if let Some(ref widget) = self.ssid {
                widgets.push(widget);
            };
            if let Some(ref widget) = self.ip_addr {
                widgets.push(widget);
            };
            widgets.push(&self.output_tx);
            if let Some(ref widget) = self.graph_tx {
                widgets.push(widget);
            }
            widgets.push(&self.output_rx);
            if let Some(ref widget) = self.graph_rx {
                widgets.push(widget);
            }
            widgets
        } else if !self.hide_inactive {
            vec![&self.network]
        } else {
            vec![]
        }
    }

    fn id(&self) -> &str {
        &self.id
    }
}

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
            let operstate = read_file(&operstate_file)?;
            Ok(operstate == "up")
        }
    }

    /// Query the device for the current `tx_bytes` statistic.
    pub fn tx_bytes(&self) -> Result<u64> {
        read_file(&self.device_path.join("statistics/tx_bytes"))?
            .parse::<u64>()
            .block_error("net", "Failed to parse tx_bytes")
    }

    /// Query the device for the current `rx_bytes` statistic.
    pub fn rx_bytes(&self) -> Result<u64> {
        read_file(&self.device_path.join("statistics/rx_bytes"))?
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
        let up = self.is_up()?;
        if !self.wireless || !up {
            return Err(BlockError(
                "net".to_string(),
                "SSIDs are only available for connected wireless devices."
                    .to_string(),
            ));
        }
        let mut iw_output = Command::new("sh")
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
            .block_error("net", "Failed to execute SSID query.")?
            .stdout;

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
        let mut ip_output = Command::new("sh")
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
            .block_error("net", "Failed to execute IP address query.")?
            .stdout;

        if ip_output.len() == 0 {
            Ok(None)
        } else {
            ip_output.pop(); // Remove trailing newline.
            String::from_utf8(ip_output)
                .block_error("net", "Non-UTF8 IP address.")
                .map(|s| Some(s))
        }
    }

    /// Queries the bitrate of this device (using `iwlist`)
    pub fn bitrate(&self) -> Result<Option<String>> {
        let up = self.is_up()?;
        if !self.wireless || !up {
            return Err(BlockError(
                "net".to_string(),
                "Bitrate is only available for connected wireless devices."
                    .to_string(),
            ));
        }
        let mut bitrate_output = Command::new("sh")
            .args(
                &[
                    "-c",
                    &format!(
                        "iw dev {} link | grep \"tx bitrate\" | awk '{{print $3\" \"$4}}'",
                        self.device
                    ),
                ],
            )
            .output()
            .block_error("net", "Failed to execute bitrate query.")?
            .stdout;

        if bitrate_output.len() == 0 {
            Ok(None)
        } else {
            bitrate_output.pop(); // Remove trailing newline.
            String::from_utf8(bitrate_output)
                .block_error("net", "Non-UTF8 bitrate.")
                .map(|s| Some(s))
        }
    }
}

pub struct Net {
    network: TextWidget,
    ssid: Option<TextWidget>,
    max_ssid_width: usize,
    ip_addr: Option<TextWidget>,
    bitrate: Option<TextWidget>,
    output_tx: Option<TextWidget>,
    graph_tx: Option<GraphWidget>,
    output_rx: Option<TextWidget>,
    graph_rx: Option<GraphWidget>,
    id: String,
    update_interval: Duration,
    device: NetworkDevice,
    tx_buff: Vec<u64>,
    rx_buff: Vec<u64>,
    tx_bytes: u64,
    rx_bytes: u64,
    active: bool,
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

    /// Whether to show the SSID of active wireless networks.
    #[serde(default = "NetConfig::default_ssid")]
    pub ssid: bool,

    /// Max SSID width, in characters.
    #[serde(default = "NetConfig::default_max_ssid_width")]
    pub max_ssid_width: usize,

    /// Whether to show the bitrate of active wireless networks.
    #[serde(default = "NetConfig::default_bitrate")]
    pub bitrate: bool,

    /// Whether to show the IP address of active networks.
    #[serde(default = "NetConfig::default_ip")]
    pub ip: bool,

    /// Whether to hide networks that are down/inactive completely.
    #[serde(default = "NetConfig::default_hide_inactive")]
    pub hide_inactive: bool,

    /// Whether to show the upload throughput indicator of active networks.
    #[serde(default = "NetConfig::default_speed_up")]
    pub speed_up: bool,

    /// Whether to show the download throughput indicator of active networks.
    #[serde(default = "NetConfig::default_speed_down")]
    pub speed_down: bool,

    /// Whether to show the upload throughput graph of active networks.
    #[serde(default = "NetConfig::default_graph_up")]
    pub graph_up: bool,

    /// Whether to show the download throughput graph of active networks.
    #[serde(default = "NetConfig::default_graph_down")]
    pub graph_down: bool,
}

impl NetConfig {
    fn default_interval() -> Duration {
        Duration::from_secs(1)
    }

    fn default_device() -> String {
        "lo".to_string()
    }

    fn default_hide_inactive() -> bool {
        false
    }

    fn default_max_ssid_width() -> usize {
        21
    }

    fn default_ssid() -> bool {
        false
    }

    fn default_bitrate() -> bool {
        false
    }

    fn default_ip() -> bool {
        false
    }

    fn default_speed_up() -> bool {
        true
    }

    fn default_speed_down() -> bool {
        true
    }

    fn default_graph_up() -> bool {
        false
    }

    fn default_graph_down() -> bool {
        false
    }
}

impl ConfigBlock for Net {
    type Config = NetConfig;

    fn new(block_config: Self::Config, config: Config, _tx_update_request: Sender<Task>) -> Result<Self> {
        let device = NetworkDevice::from_device(block_config.device)?;
        let init_rx_bytes = device.rx_bytes()?;
        let init_tx_bytes = device.tx_bytes()?;
        let wireless = device.is_wireless();
        Ok(Net {
            id: format!("{}", Uuid::new_v4().to_simple()),
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
            max_ssid_width: block_config.max_ssid_width,
            bitrate: match block_config.bitrate {
                true => Some(TextWidget::new(config.clone())),
                false => None,
            },
            ip_addr: match block_config.ip {
                true => Some(TextWidget::new(config.clone())),
                false => None,
            },
            output_tx: match block_config.speed_up {
                true => Some(TextWidget::new(config.clone()).with_icon("net_up")),
                false => None,
            },
            output_rx: match block_config.speed_down {
                true => Some(TextWidget::new(config.clone()).with_icon("net_down")),
                false => None,
            },
            graph_tx: match block_config.graph_up {
                true => Some(GraphWidget::new(config.clone())),
                false => None,
            },
            graph_rx: match block_config.graph_down {
                true => Some(GraphWidget::new(config.clone())),
                false => None,
            },
            device: device,
            rx_buff: vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            tx_buff: vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            rx_bytes: init_rx_bytes,
            tx_bytes: init_tx_bytes,
            active: true,
            hide_inactive: block_config.hide_inactive,
            last_update: Instant::now() - Duration::from_secs(30),
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
        let is_up = self.device.is_up()?;
        if !is_up {
            self.active = false;
            self.network.set_text("×".to_string());
            if let Some(ref mut tx_widget) = self.output_tx {
                tx_widget.set_text("×".to_string());
            };
            if let Some(ref mut rx_widget) = self.output_rx {
                rx_widget.set_text("×".to_string());
            };

            return Ok(Some(self.update_interval));
        } else {
            self.active = true;
            self.network.set_text("".to_string());
        }

        // Update SSID and IP address every 30s and the bitrate every 10s
        let now = Instant::now();
        if now.duration_since(self.last_update).as_secs() % 10 == 0 {
            if let Some(ref mut bitrate_widget) = self.bitrate {
                let bitrate = self.device.bitrate()?;
                if bitrate.is_some() {
                    bitrate_widget.set_text(bitrate.unwrap());
                }
            }
        }
        if now.duration_since(self.last_update).as_secs() > 30 {
            if let Some(ref mut ssid_widget) = self.ssid {
                let ssid = self.device.ssid()?;
                if ssid.is_some() {
                    let mut truncated = ssid.unwrap();
                    truncated.truncate(self.max_ssid_width);
                    ssid_widget.set_text(truncated);
                }
            }
            if let Some(ref mut ip_addr_widget) = self.ip_addr {
                let ip_addr = self.device.ip_addr()?;
                if ip_addr.is_some() {
                    ip_addr_widget.set_text(ip_addr.unwrap());
                }
            }
            self.last_update = now;
        }

        // Update the throughout/graph widgets if they are enabled
        let update_interval = (self.update_interval.as_secs() as f64) + (self.update_interval.subsec_nanos() as f64 / 1_000_000_000.0);
        if self.output_tx.is_some() || self.graph_tx.is_some() {
            let current_tx = self.device.tx_bytes()?;
            let tx_bytes = ((current_tx - self.tx_bytes) as f64 / update_interval) as u64;
            let (tx_speed, tx_unit) = convert_speed(tx_bytes);
            self.tx_bytes = current_tx;

            if let Some(ref mut tx_widget) = self.output_tx {
                tx_widget.set_text(format!("{:5.1}{}", tx_speed, tx_unit));
            };

            if let Some(ref mut graph_tx_widget) = self.graph_tx {
                self.tx_buff.remove(0);
                self.tx_buff.push(tx_bytes);
                graph_tx_widget.set_values(&self.tx_buff, None, None);
            }
        }
        if self.output_rx.is_some() || self.graph_rx.is_some() {
            let current_rx = self.device.rx_bytes()?;
            let rx_bytes = ((current_rx - self.rx_bytes) as f64 / update_interval) as u64;
            let (rx_speed, rx_unit) = convert_speed(rx_bytes);
            self.rx_bytes = current_rx;

            if let Some(ref mut rx_widget) = self.output_rx {
                rx_widget.set_text(format!("{:5.1}{}", rx_speed, rx_unit));
            };

            if let Some(ref mut graph_rx_widget) = self.graph_rx {
                self.rx_buff.remove(0);
                self.rx_buff.push(rx_bytes);
                graph_rx_widget.set_values(&self.rx_buff, None, None);
            }
        }

        Ok(Some(self.update_interval))
    }

    fn view(&self) -> Vec<&I3BarWidget> {
        if self.active {
            let mut widgets: Vec<&I3BarWidget> = Vec::with_capacity(7);
            widgets.push(&self.network);
            if let Some(ref ssid_widget) = self.ssid {
                widgets.push(ssid_widget);
            };
            if let Some(ref bitrate_widget) = self.bitrate {
                widgets.push(bitrate_widget);
            }
            if let Some(ref ip_addr_widget) = self.ip_addr {
                widgets.push(ip_addr_widget);
            };
            if let Some(ref tx_widget) = self.output_tx {
                widgets.push(tx_widget);
            };
            if let Some(ref graph_tx_widget) = self.graph_tx {
                widgets.push(graph_tx_widget);
            };
            if let Some(ref rx_widget) = self.output_rx {
                widgets.push(rx_widget);
            };
            if let Some(ref graph_rx_widget) = self.graph_rx {
                widgets.push(graph_rx_widget);
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

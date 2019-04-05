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
    tun: bool,
}

impl NetworkDevice {
    /// Use the network device `device`. Raises an error if a directory for that
    /// device is not found.
    pub fn from_device(device: String) -> Self {
        let device_path = Path::new("/sys/class/net").join(device.clone());

        // I don't believe that this should ever change, so set it now:
        let wireless = device_path.join("wireless").exists();
        let tun = device_path.join("tun_flags").exists();

        NetworkDevice {
            device,
            device_path,
            wireless,
            tun,
        }
    }

    /// Check whether the device exists.
    pub fn exists(&self) -> Result<bool> {
        Ok(self.device_path.exists())
    }

    /// Check whether this network device is in the `up` state. Note that a
    /// device that is not `up` is not necessarily `down`.
    pub fn is_up(&self) -> Result<bool> {
        let operstate_file = self.device_path.join("operstate");
        if !operstate_file.exists() {
            // It seems more reasonable to treat these as inactive networks as
            // opposed to erroring out the entire block.
            Ok(false)
        } else if self.tun {
            Ok(true)
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

    /// Checks whether this device is vpn network.
    pub fn is_vpn(&self) -> bool {
        self.tun
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
            .args(&[
                "-c",
                &format!(
                    "iw dev {} link | awk '/^\\s+SSID:/ {{ print $2 }}'",
                    self.device
                ),
            ])
            .output()
            .block_error("net", "Failed to execute SSID query.")?
            .stdout;

        if iw_output.is_empty() {
            iw_output = Command::new("nmcli")
                .args(&["-g", "general.connection", "device", "show", &self.device])
                .output()
                .block_error("net", "Failed to execute SSID query.")?
                .stdout;
        }

        if iw_output.is_empty() {
            Ok(None)
        } else {
            iw_output.pop(); // Remove trailing newline.
            String::from_utf8(iw_output)
                .block_error("net", "Non-UTF8 SSID.")
                .map(Some)
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

        if ip_output.is_empty() {
            Ok(None)
        } else {
            ip_output.pop(); // Remove trailing newline.
            String::from_utf8(ip_output)
                .block_error("net", "Non-UTF8 IP address.")
                .map(Some)
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
            .args(&[
                "-c",
                &format!(
                    "iw dev {} link | awk '/tx bitrate/ {{print $3\" \"$4}}'",
                    self.device
                ),
            ])
            .output()
            .block_error("net", "Failed to execute bitrate query.")?
            .stdout;

        if bitrate_output.is_empty() {
            Ok(None)
        } else {
            bitrate_output.pop(); // Remove trailing newline.
            String::from_utf8(bitrate_output)
                .block_error("net", "Non-UTF8 bitrate.")
                .map(Some)
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
    hide_missing: bool,
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

    /// Whether to hide networks that are missing.
    #[serde(default = "NetConfig::default_hide_missing")]
    pub hide_missing: bool,

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

    fn default_hide_missing() -> bool {
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
        let device = NetworkDevice::from_device(block_config.device);
        let init_rx_bytes = device.rx_bytes().unwrap_or(0);
        let init_tx_bytes = device.tx_bytes().unwrap_or(0);
        let wireless = device.is_wireless();
        let vpn = device.is_vpn();
        Ok(Net {
            id: Uuid::new_v4().simple().to_string(),
            update_interval: block_config.interval,
            network: TextWidget::new(config.clone()).with_icon(if wireless {
                "net_wireless" } else if vpn {
                "net_vpn" } else {
                "net_wired"
            }),
            // Might want to signal an error if the user wants the SSID of a
            // wired connection instead.
            ssid: if block_config.ssid && wireless {
                Some(TextWidget::new(config.clone())) } else {
                None
            },
            max_ssid_width: block_config.max_ssid_width,
            bitrate: if block_config.bitrate {
                Some(TextWidget::new(config.clone())) } else {
                None
            },
            ip_addr: if block_config.ip {
                Some(TextWidget::new(config.clone())) } else {
                None
            },
            output_tx: if block_config.speed_up {
                Some(TextWidget::new(config.clone()).with_icon("net_up")) } else {
                None
            },
            output_rx: if block_config.speed_down {
                Some(TextWidget::new(config.clone()).with_icon("net_down")) } else {
                None
            },
            graph_tx: if block_config.graph_up {
                Some(GraphWidget::new(config.clone())) } else { 
                None
            },
            graph_rx: if block_config.graph_down {
                Some(GraphWidget::new(config.clone())) } else {
                None
            },
            device,
            rx_buff: vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            tx_buff: vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            rx_bytes: init_rx_bytes,
            tx_bytes: init_tx_bytes,
            active: true,
            hide_inactive: block_config.hide_inactive,
            hide_missing: block_config.hide_missing,
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
        let exists = self.device.exists()?;
        let is_up = self.device.is_up()?;
        if !exists || !is_up {
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

        // TODO: consider using `as_nanos`
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
        } else if !self.hide_inactive || !self.hide_missing {
            vec![&self.network]
        } else {
            vec![]
        }
    }

    fn id(&self) -> &str {
        &self.id
    }
}

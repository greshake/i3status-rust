use std::fs::read_to_string;
use std::fs::OpenOptions;
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use serde_derive::Deserialize;
use uuid::Uuid;

use crate::blocks::{Block, ConfigBlock};
use crate::config::Config;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::input::{I3BarEvent, MouseButton};
use crate::scheduler::Task;
use crate::subprocess::spawn_child_async;
use crate::util::format_percent_bar;
use crate::widget::I3BarWidget;
use crate::widgets::button::ButtonWidget;
use crate::widgets::graph::GraphWidget;

pub struct NetworkDevice {
    device: String,
    device_path: PathBuf,
    wireless: bool,
    tun: bool,
    wg: bool,
    ppp: bool,
}

impl NetworkDevice {
    /// Use the network device `device`. Raises an error if a directory for that
    /// device is not found.
    pub fn from_device(device: String) -> Self {
        let device_path = Path::new("/sys/class/net").join(device.clone());

        // I don't believe that this should ever change, so set it now:
        let wireless = device_path.join("wireless").exists();
        let tun = device_path.join("tun_flags").exists()
            || device.starts_with("tun")
            || device.starts_with("tap");

        let uevent_path = device_path.join("uevent");
        let uevent_content = read_to_string(&uevent_path);

        let wg = match &uevent_content {
            Ok(s) => s.contains("wireguard"),
            Err(_e) => false,
        };
        let ppp = match &uevent_content {
            Ok(s) => s.contains("ppp"),
            Err(_e) => false,
        };

        NetworkDevice {
            device,
            device_path,
            wireless,
            tun,
            wg,
            ppp,
        }
    }

    pub fn device(&self) -> String {
        self.device.clone()
    }

    /// Grab the name of the 'default' device.
    /// A default device is usually selected by the network manager
    /// and will change when the status of devices change.
    pub fn default_device() -> Option<String> {
        String::from_utf8(
            Command::new("sh")
                .args(&[
                    "-c",
                    "ip route show default|head -n1|sed -n 's/^default.*dev \\(\\w*\\).*/\\1/p'",
                ])
                .output()
                .ok()
                .map(|o| {
                    let mut v = o.stdout;
                    v.pop(); // remove newline
                    v
                })?,
        )
        .ok()
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
        } else if self.tun || self.wg || self.ppp {
            Ok(true)
        } else {
            let operstate = read_file(&operstate_file)?;
            let carrier_file = self.device_path.join("carrier");
            if !carrier_file.exists() {
                Ok(operstate == "up")
            } else {
                if operstate == "up" {
                    Ok(true)
                } else {
                    let carrier = read_file(&carrier_file);
                    match carrier {
                        Ok(carrier) => Ok(carrier == "1"),
                        Err(_e) => Ok(operstate == "up"),
                    }
                }
            }
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
        self.tun || self.wg || self.ppp
    }

    /// Queries the wireless SSID of this device (using `iw`), if it is
    /// connected to one.
    pub fn ssid(&self) -> Result<Option<String>> {
        let up = self.is_up()?;
        if !self.wireless || !up {
            return Err(BlockError(
                "net".to_string(),
                "SSIDs are only available for connected wireless devices.".to_string(),
            ));
        }
        let mut iw_output = Command::new("sh")
            .args(&[
                "-c",
                &format!(
                    "iw dev {} link | sed -n 's/^\\s\\+SSID: \\(.*\\)/\\1/p'",
                    self.device
                ),
            ])
            .output()
            .block_error("net", "Failed to execute SSID query using iw.")?
            .stdout;

        if iw_output.is_empty() {
            iw_output = Command::new("sh")
                .args(&[
                    "-c",
                    &format!(
                        "wpa_cli status -i{} | sed -n 's/^ssid=\\(.*\\)/\\1/p'",
                        self.device
                    ),
                ])
                .output()
                .block_error("net", "Failed to execute SSID query using wpa_cli.")?
                .stdout;
        }

        if iw_output.is_empty() {
            iw_output = Command::new("nmcli")
                .args(&["-g", "general.connection", "device", "show", &self.device])
                .output()
                .block_error("net", "Failed to execute SSID query using nmcli.")?
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

    fn absolute_signal_strength(&self) -> Result<Option<i32>> {
        let up = self.is_up()?;
        if !self.wireless || !up {
            return Err(BlockError(
                "net".to_string(),
                "Signal strength is only available for connected wireless devices.".to_string(),
            ));
        }
        let mut iw_output = Command::new("sh")
            .args(&[
                "-c",
                &format!(
                    "iw dev {} link | sed -n 's/^\\s\\+signal: \\(.*\\) dBm/\\1/p'",
                    self.device
                ),
            ])
            .output()
            .block_error("net", "Failed to execute signal strength query.")?
            .stdout;
        if iw_output.is_empty() {
            Ok(None)
        } else {
            iw_output.pop(); // Remove trailing newline.
            String::from_utf8(iw_output)
                .block_error("net", "Non-UTF8 signal strength.")
                .and_then(|as_str| {
                    as_str
                        .parse::<i32>()
                        .block_error("net", "Non numerical signal strength.")
                })
                .map(Some)
        }
    }

    fn relative_signal_strength(&self) -> Result<Option<u32>> {
        let xbm = if let Some(xbm) = self.absolute_signal_strength()? {
            xbm as f64
        } else {
            return Ok(None);
        };

        // Code inspired by https://github.com/NetworkManager/NetworkManager/blob/master/src/platform/wifi/nm-wifi-utils-nl80211.c
        const NOISE_FLOOR_DBM: f64 = -90.;
        const SIGNAL_MAX_DBM: f64 = -20.;

        let xbm = if xbm < NOISE_FLOOR_DBM {
            NOISE_FLOOR_DBM
        } else if xbm > SIGNAL_MAX_DBM {
            SIGNAL_MAX_DBM
        } else {
            xbm
        };

        let result = 100. - 70. * ((SIGNAL_MAX_DBM - xbm) / (SIGNAL_MAX_DBM - NOISE_FLOOR_DBM));
        let result = result as u32;
        Ok(Some(result))
    }

    /// Queries the inet IP of this device (using `ip`).
    pub fn ip_addr(&self) -> Result<Option<String>> {
        if !self.is_up()? {
            return Ok(None);
        }
        let mut ip_output = Command::new("sh")
            .args(&[
                "-c",
                &format!(
                    "ip -oneline -family inet address show {} | sed -rn -e \"s/.*inet ([\\.0-9/]+).*/\\1/; G; s/\\n/ /;h\" -e \"$ P;\"",
                    self.device
                ),
            ])
            .output()
            .block_error("net", "Failed to execute IP address query.")?
            .stdout;

        if ip_output.is_empty() {
            Ok(None)
        } else {
            ip_output.pop(); // Remove trailing newline.
            let ip = String::from_utf8(ip_output)
                .block_error("net", "Non-UTF8 IP address.")?
                .trim()
                .to_string();
            Ok(Some(ip))
        }
    }

    /// Queries the inet IPv6 of this device (using `ip`).
    pub fn ipv6_addr(&self) -> Result<Option<String>> {
        if !self.is_up()? {
            return Ok(None);
        }

        let mut ip_output = Command::new("sh")
            .args(&[
                "-c",
                &format!(
                    "ip -oneline -family inet6 address show {} | sed -e 's/^.*inet6 \\([^ ]\\+\\).*/\\1/'",
                    self.device
                ),
            ])
            .output()
            .block_error("net", "Failed to execute IPv6 address query.")?
            .stdout;

        if ip_output.is_empty() {
            Ok(None)
        } else {
            ip_output.pop(); // Remove trailing newline.
            let ip = String::from_utf8(ip_output)
                .block_error("net", "Non-UTF8 IP address.")?
                .trim()
                .to_string();
            Ok(Some(ip))
        }
    }

    /// Queries the bitrate of this device (using `iwlist`)
    pub fn bitrate(&self) -> Result<Option<String>> {
        let up = self.is_up()?;
        if !up {
            return Err(BlockError(
                "net".to_string(),
                "Bitrate is only available for connected devices.".to_string(),
            ));
        }
        let command = if self.wireless {
            format!(
                "iw dev {} link | awk '/tx bitrate/ {{print $3\" \"$4}}'",
                self.device
            )
        } else {
            format!(
                "ethtool {} 2>/dev/null | awk '/Speed:/ {{print $2}}'",
                self.device
            )
        };
        let mut bitrate_output = Command::new("sh")
            .args(&["-c", &command])
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
    network: ButtonWidget,
    ssid: Option<ButtonWidget>,
    max_ssid_width: usize,
    signal_strength: Option<ButtonWidget>,
    signal_strength_bar: bool,
    ip_addr: Option<ButtonWidget>,
    ipv6_addr: Option<ButtonWidget>,
    bitrate: Option<ButtonWidget>,
    output_tx: Option<ButtonWidget>,
    graph_tx: Option<GraphWidget>,
    output_rx: Option<ButtonWidget>,
    graph_rx: Option<GraphWidget>,
    id: String,
    update_interval: Duration,
    device: NetworkDevice,
    auto_device: bool,
    tx_buff: Vec<u64>,
    rx_buff: Vec<u64>,
    tx_bytes: u64,
    rx_bytes: u64,
    use_bits: bool,
    active: bool,
    hide_inactive: bool,
    hide_missing: bool,
    last_update: Instant,
    on_click: Option<String>,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct NetConfig {
    /// Update interval in seconds
    #[serde(
        default = "NetConfig::default_interval",
        deserialize_with = "deserialize_duration"
    )]
    pub interval: Duration,

    /// Which interface in /sys/class/net/ to read from.
    #[serde(default = "NetConfig::default_device")]
    pub device: String,

    #[serde(default = "NetConfig::default_auto_device")]
    pub auto_device: bool,

    /// Whether to show the SSID of active wireless networks.
    #[serde(default = "NetConfig::default_ssid")]
    pub ssid: bool,

    /// Max SSID width, in characters.
    #[serde(default = "NetConfig::default_max_ssid_width")]
    pub max_ssid_width: usize,

    /// Whether to show the signal strength of active wireless networks.
    #[serde(default = "NetConfig::default_signal_strength")]
    pub signal_strength: bool,

    /// Whether to show the signal strength of active wireless networks as a bar.
    #[serde(default = "NetConfig::default_signal_strength_bar")]
    pub signal_strength_bar: bool,

    /// Whether to show the bitrate of active wireless networks.
    #[serde(default = "NetConfig::default_bitrate")]
    pub bitrate: bool,

    /// Whether to show the IP address of active networks.
    #[serde(default = "NetConfig::default_ip")]
    pub ip: bool,

    /// Whether to show the IPv6 address of active networks.
    #[serde(default = "NetConfig::default_ipv6")]
    pub ipv6: bool,

    /// Whether to hide networks that are down/inactive completely.
    #[serde(default = "NetConfig::default_hide_inactive")]
    pub hide_inactive: bool,

    /// Whether to hide networks that are missing.
    #[serde(default = "NetConfig::default_hide_missing")]
    pub hide_missing: bool,

    /// Whether to show the upload throughput indicator of active networks.
    #[serde(default = "NetConfig::default_speed_up")]
    pub speed_up: bool,

    /// Whether to show speeds in bits or bytes per second.
    #[serde(default = "NetConfig::default_use_bits")]
    pub use_bits: bool,

    /// Whether to show the download throughput indicator of active networks.
    #[serde(default = "NetConfig::default_speed_down")]
    pub speed_down: bool,

    /// Whether to show the upload throughput graph of active networks.
    #[serde(default = "NetConfig::default_graph_up")]
    pub graph_up: bool,

    /// Whether to show the download throughput graph of active networks.
    #[serde(default = "NetConfig::default_graph_down")]
    pub graph_down: bool,

    #[serde(default = "NetConfig::default_on_click")]
    pub on_click: Option<String>,
}

impl NetConfig {
    fn default_interval() -> Duration {
        Duration::from_secs(1)
    }

    fn default_device() -> String {
        match NetworkDevice::default_device() {
            Some(ref s) if !s.is_empty() => s.to_string(),
            _ => "lo".to_string(),
        }
    }

    fn default_auto_device() -> bool {
        false
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

    fn default_signal_strength() -> bool {
        false
    }

    fn default_signal_strength_bar() -> bool {
        false
    }

    fn default_bitrate() -> bool {
        false
    }

    fn default_ip() -> bool {
        false
    }

    fn default_ipv6() -> bool {
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

    fn default_use_bits() -> bool {
        false
    }

    fn default_on_click() -> Option<String> {
        None
    }
}

impl ConfigBlock for Net {
    type Config = NetConfig;

    fn new(
        block_config: Self::Config,
        config: Config,
        _tx_update_request: Sender<Task>,
    ) -> Result<Self> {
        let device = NetworkDevice::from_device(block_config.device);
        let init_rx_bytes = device.rx_bytes().unwrap_or(0);
        let init_tx_bytes = device.tx_bytes().unwrap_or(0);
        let wireless = device.is_wireless();
        let vpn = device.is_vpn();
        let id = Uuid::new_v4().to_simple().to_string();
        Ok(Net {
            id: id.clone(),
            update_interval: block_config.interval,
            use_bits: block_config.use_bits,
            network: ButtonWidget::new(config.clone(), &id).with_icon(if wireless {
                "net_wireless"
            } else if vpn {
                "net_vpn"
            } else {
                "net_wired"
            }),
            // Might want to signal an error if the user wants the SSID of a
            // wired connection instead.
            ssid: if block_config.ssid && wireless {
                Some(ButtonWidget::new(config.clone(), &id).with_text(" "))
            } else {
                None
            },
            max_ssid_width: block_config.max_ssid_width,
            signal_strength: if block_config.signal_strength && wireless {
                Some(ButtonWidget::new(config.clone(), &id))
            } else {
                None
            },
            signal_strength_bar: block_config.signal_strength_bar,
            bitrate: if block_config.bitrate {
                Some(ButtonWidget::new(config.clone(), &id))
            } else {
                None
            },
            ip_addr: if block_config.ip {
                Some(ButtonWidget::new(config.clone(), &id))
            } else {
                None
            },
            ipv6_addr: if block_config.ipv6 {
                Some(ButtonWidget::new(config.clone(), &id))
            } else {
                None
            },
            output_tx: if block_config.speed_up {
                Some(ButtonWidget::new(config.clone(), &id).with_icon("net_up"))
            } else {
                None
            },
            output_rx: if block_config.speed_down {
                Some(ButtonWidget::new(config.clone(), &id).with_icon("net_down"))
            } else {
                None
            },
            graph_tx: if block_config.graph_up {
                Some(GraphWidget::new(config.clone()))
            } else {
                None
            },
            graph_rx: if block_config.graph_down {
                Some(GraphWidget::new(config.clone()))
            } else {
                None
            },
            device,
            auto_device: block_config.auto_device,
            rx_buff: vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            tx_buff: vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            rx_bytes: init_rx_bytes,
            tx_bytes: init_tx_bytes,
            active: true,
            hide_inactive: block_config.hide_inactive,
            hide_missing: block_config.hide_missing,
            last_update: Instant::now() - Duration::from_secs(30),
            on_click: block_config.on_click,
        })
    }
}

fn read_file(path: &Path) -> Result<String> {
    let mut f = OpenOptions::new().read(true).open(path).block_error(
        "net",
        &format!("failed to open file {}", path.to_string_lossy()),
    )?;
    let mut content = String::new();
    f.read_to_string(&mut content)
        .block_error("net", &format!("failed to read {}", path.to_string_lossy()))?;
    // Removes trailing newline
    content.pop();
    Ok(content)
}

fn convert_speed(speed: u64, use_bits: bool) -> (f64, &'static str) {
    let mut multiplier = 1;
    let b = if use_bits {
        multiplier = 8;
        "b"
    } else {
        "B"
    };
    // the values for the match are so the speed doesn't go above 3 characters
    let (speed, unit) = match speed {
        x if (x * multiplier) > 999_999_999 => (speed as f64 / 1_000_000_000.0, "G"),
        x if (x * multiplier) > 999_999 => (speed as f64 / 1_000_000.0, "M"),
        x if (x * multiplier) > 999 => (speed as f64 / 1_000.0, "k"),
        _ => (speed as f64, b),
    };
    (speed, unit)
}

impl Block for Net {
    fn update(&mut self) -> Result<Option<Duration>> {
        if self.auto_device {
            // update the device and icon to the device currently marked as default
            let dev = NetConfig::default_device();
            if self.device.device() != dev {
                self.device = NetworkDevice::from_device(dev);
                self.network.set_icon(if self.device.is_wireless() {
                    "net_wireless"
                } else if self.device.is_vpn() {
                    "net_vpn"
                } else {
                    "net_wired"
                });
            }
        }
        // Skip updating tx/rx if device is not up.
        let exists = self.device.exists()?;
        let is_up = self.device.is_up()?;
        if !exists || !is_up {
            self.active = false;
            self.network.set_text(" ×".to_string());
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
                if let Some(b) = bitrate {
                    bitrate_widget.set_text(b);
                }
            }
        }
        if now.duration_since(self.last_update).as_secs() > 30 {
            if let Some(ref mut ssid_widget) = self.ssid {
                let ssid = self.device.ssid()?;
                if let Some(s) = ssid {
                    let mut truncated = s;
                    truncated.truncate(self.max_ssid_width);
                    ssid_widget.set_text(truncated);
                }
            }
            if let Some(ref mut signal_strength_widget) = self.signal_strength {
                let value = self.device.relative_signal_strength()?;
                if let Some(v) = value {
                    signal_strength_widget.set_text(if self.signal_strength_bar {
                        format_percent_bar(v as f32)
                    } else {
                        format!("{}%", v)
                    });
                }
            }
            if let Some(ref mut ip_addr_widget) = self.ip_addr {
                let ip_addr = self.device.ip_addr()?;
                if let Some(ip) = ip_addr {
                    ip_addr_widget.set_text(ip);
                }
            }
            if let Some(ref mut ipv6_addr_widget) = self.ipv6_addr {
                let ipv6_addr = self.device.ipv6_addr()?;
                if let Some(ip) = ipv6_addr {
                    ipv6_addr_widget.set_text(ip);
                }
            }
            self.last_update = now;
        }

        // allow us to display bits or bytes
        // dependent on user's config setting
        let multiplier = if self.use_bits { 8.0 } else { 1.0 };
        // TODO: consider using `as_nanos`
        // Update the throughout/graph widgets if they are enabled
        let update_interval = (self.update_interval.as_secs() as f64)
            + (self.update_interval.subsec_nanos() as f64 / 1_000_000_000.0);
        if self.output_tx.is_some() || self.graph_tx.is_some() {
            let current_tx = self.device.tx_bytes()?;
            let tx_bytes = ((current_tx - self.tx_bytes) as f64 / update_interval) as u64;
            let (tx_speed, tx_unit) = convert_speed(tx_bytes, self.use_bits);
            self.tx_bytes = current_tx;

            if let Some(ref mut tx_widget) = self.output_tx {
                tx_widget.set_text(format!("{:5.1}{}", tx_speed * multiplier, tx_unit));
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
            let (rx_speed, rx_unit) = convert_speed(rx_bytes, self.use_bits);
            self.rx_bytes = current_rx;

            if let Some(ref mut rx_widget) = self.output_rx {
                rx_widget.set_text(format!("{:5.1}{}", rx_speed * multiplier, rx_unit));
            };

            if let Some(ref mut graph_rx_widget) = self.graph_rx {
                self.rx_buff.remove(0);
                self.rx_buff.push(rx_bytes);
                graph_rx_widget.set_values(&self.rx_buff, None, None);
            }
        }

        Ok(Some(self.update_interval))
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        if self.active {
            let mut widgets: Vec<&dyn I3BarWidget> = Vec::with_capacity(8);
            widgets.push(&self.network);
            if let Some(ref ssid_widget) = self.ssid {
                widgets.push(ssid_widget);
            };
            if let Some(ref signal_strength_widget) = self.signal_strength {
                widgets.push(signal_strength_widget);
            };
            if let Some(ref bitrate_widget) = self.bitrate {
                widgets.push(bitrate_widget);
            }
            if let Some(ref ip_addr_widget) = self.ip_addr {
                widgets.push(ip_addr_widget);
            };
            if let Some(ref ipv6_addr_widget) = self.ipv6_addr {
                widgets.push(ipv6_addr_widget);
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

    fn click(&mut self, e: &I3BarEvent) -> Result<()> {
        if let Some(ref name) = e.name {
            if name.as_str() == self.id {
                match e.button {
                    MouseButton::Left => match self.on_click {
                        Some(ref cmd) => {
                            spawn_child_async("sh", &["-c", cmd])
                                .block_error("net", "could not spawn child")?;
                        }
                        _ => (),
                    },
                    _ => (),
                }
            }
        }

        Ok(())
    }

    fn id(&self) -> &str {
        &self.id
    }
}

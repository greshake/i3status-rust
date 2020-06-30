use std::fmt;
use std::fs::read_to_string;
use std::fs::OpenOptions;
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use serde_derive::Deserialize;
use uuid::Uuid;

use crate::blocks::Update;
use crate::blocks::{Block, ConfigBlock};
use crate::config::Config;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::input::{I3BarEvent, MouseButton};
use crate::scheduler::Task;
use crate::subprocess::spawn_child_async;
use crate::util::{
    escape_pango_text, format_percent_bar, format_speed, format_vec_to_bar_graph, FormatTemplate,
};
use crate::widget::I3BarWidget;
use crate::widgets::button::ButtonWidget;

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
            } else if operstate == "up" {
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
            iw_output = Command::new("sh")
                .args(&[
                    "-c",
                    &format!("nmcli -g general.connection device show {}", &self.device),
                ])
                .output()
                .block_error("net", "Failed to execute SSID query using nmcli.")?
                .stdout;
        }

        if iw_output.is_empty() {
            iw_output = Command::new("sh")
                .args(&[
                    "-c",
                    &format!(
                        "iwctl station {} show | sed -n 's/^\\s\\+Connected network\\s\\(.*\\)\\s*/\\1/p'",
                        self.device
                    ),
                ])
                .output()
                .block_error("net", "Failed to execute SSID query using iwctl.")?
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
    format: FormatTemplate,
    output: ButtonWidget,
    config: Config,
    network: ButtonWidget,
    ssid: Option<String>,
    max_ssid_width: usize,
    signal_strength: Option<String>,
    signal_strength_bar: Option<String>,
    ip_addr: Option<String>,
    ipv6_addr: Option<String>,
    bitrate: Option<String>,
    output_tx: Option<String>,
    graph_tx: Option<String>,
    output_rx: Option<String>,
    graph_rx: Option<String>,
    id: String,
    update_interval: Duration,
    device: NetworkDevice,
    auto_device: bool,
    tx_buff: Vec<u64>,
    rx_buff: Vec<u64>,
    tx_bytes: u64,
    rx_bytes: u64,
    use_bits: bool,
    speed_min_unit: Unit,
    speed_digits: usize,
    active: bool,
    hide_inactive: bool,
    hide_missing: bool,
    last_update: Instant,
    on_click: Option<String>,
}

#[derive(Copy, Clone, Debug, Deserialize)]
pub enum Unit {
    B,
    K,
    M,
    G,
    T,
}

impl Default for Unit {
    fn default() -> Self {
        Unit::K
    }
}

impl fmt::Display for Unit {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
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

    #[serde(default = "NetConfig::default_format")]
    pub format: String,

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

    /// Number of digits to show for throughput indiciators.
    #[serde(default = "NetConfig::default_speed_digits")]
    pub speed_digits: usize,

    /// Minimum unit to display for throughput indicators.
    #[serde(default = "NetConfig::default_speed_min_unit")]
    pub speed_min_unit: Unit,

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

    fn default_format() -> String {
        "{speed_up} {speed_down}".to_owned()
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

    fn default_speed_min_unit() -> Unit {
        Unit::K
    }

    fn default_speed_digits() -> usize {
        3
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

        // Detecting deprecated options, If any deprecated options is used, format string option will not be used.
        let mut old_user_format = Vec::new();
        let old_strings = [
            "ssid",
            "signal_strength",
            "signal_strength_bar",
            "bitrate",
            "ip",
            "ipv6",
            "speed_up",
            "speed_down",
            "graph_up",
            "graph_down",
        ];
        for string in old_strings.iter() {
            if config.blocks[0].1.get(string.to_owned()).is_some() {
                old_user_format.push(format!("{{{}}}", string));
            };
        }

        let old_format = old_user_format.join(" ");

        Ok(Net {
            id: id.clone(),
            update_interval: block_config.interval,
            format: FormatTemplate::from_string(if old_user_format.is_empty() {
                &block_config.format
            } else {
                &old_format
            })
            .block_error("net", "Invalid format specified")?,
            output: ButtonWidget::new(config.clone(), "").with_text(""),
            config: config.clone(),
            use_bits: block_config.use_bits,
            speed_min_unit: block_config.speed_min_unit,
            speed_digits: block_config.speed_digits,
            network: ButtonWidget::new(config, &id).with_icon(if wireless {
                "net_wireless"
            } else if vpn {
                "net_vpn"
            } else {
                "net_wired"
            }),
            // Might want to signal an error if the user wants the SSID of a
            // wired connection instead.
            ssid: if wireless {
                Some(" ".to_string())
            } else {
                None
            },
            max_ssid_width: block_config.max_ssid_width,
            signal_strength: if wireless { Some(0.to_string()) } else { None },
            signal_strength_bar: if wireless { Some("".to_string()) } else { None },
            bitrate: Some("".to_string()),
            ip_addr: Some("".to_string()),
            ipv6_addr: Some("".to_string()),
            output_tx: Some("".to_string()),
            output_rx: Some("".to_string()),
            graph_tx: Some("".to_string()),
            graph_rx: Some("".to_string()),
            device,
            auto_device: block_config.auto_device,
            rx_buff: vec![0; 10],
            tx_buff: vec![0; 10],
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

impl Net {
    fn update_device(&mut self) {
        if self.auto_device {
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
    }

    fn update_bitrate(&mut self) -> Result<()> {
        if let Some(ref mut bitrate_string) = self.bitrate {
            let bitrate = self.device.bitrate()?;
            if let Some(b) = bitrate {
                *bitrate_string = b;
            }
        }
        Ok(())
    }

    fn update_ssid(&mut self) -> Result<()> {
        if let Some(ref mut ssid_string) = self.ssid {
            let ssid = self.device.ssid()?;
            if let Some(s) = ssid {
                let mut truncated = s;
                truncated.truncate(self.max_ssid_width);
                // SSID names can contain chars that need escaping
                *ssid_string = escape_pango_text(truncated);
            }
        }
        Ok(())
    }

    fn update_signal_strength(&mut self) -> Result<()> {
        if self.signal_strength.is_some() || self.signal_strength_bar.is_some() {
            let value = self.device.relative_signal_strength()?;
            if let Some(ref mut signal_strength_string) = self.signal_strength {
                if let Some(v) = value {
                    *signal_strength_string = format!("{}%", v);
                };
            }

            if let Some(ref mut signal_strength_bar_string) = self.signal_strength_bar {
                if let Some(v) = value {
                    *signal_strength_bar_string = format_percent_bar(v as f32);
                };
            }
        }
        Ok(())
    }

    fn update_ip_addr(&mut self) -> Result<()> {
        if let Some(ref mut ip_addr_string) = self.ip_addr {
            let ip_addr = self.device.ip_addr()?;
            if let Some(ip) = ip_addr {
                *ip_addr_string = ip;
            }
        }
        if let Some(ref mut ipv6_addr_string) = self.ipv6_addr {
            let ipv6_addr = self.device.ipv6_addr()?;
            if let Some(ip) = ipv6_addr {
                *ipv6_addr_string = ip;
            }
        }
        Ok(())
    }

    fn update_tx_rx(&mut self) -> Result<()> {
        // TODO: consider using `as_nanos`
        let update_interval = (self.update_interval.as_secs() as f64)
        // Update the throughput/graph widgets if they are enabled
            + (self.update_interval.subsec_nanos() as f64 / 1_000_000_000.0);
        if self.output_tx.is_some() || self.graph_tx.is_some() {
            let current_tx = self.device.tx_bytes()?;
            let tx_bytes = ((current_tx - self.tx_bytes) as f64 / update_interval) as u64;
            self.tx_bytes = current_tx;

            if let Some(ref mut tx) = self.output_tx {
                *tx = format_speed(
                    tx_bytes,
                    self.speed_digits,
                    &self.speed_min_unit.to_string(),
                    self.use_bits,
                );
            };

            if let Some(ref mut graph_tx) = self.graph_tx {
                self.tx_buff.remove(0);
                self.tx_buff.push(tx_bytes);
                *graph_tx = format_vec_to_bar_graph(&self.tx_buff, None, None);
            }
        }
        if self.output_rx.is_some() || self.graph_rx.is_some() {
            let current_rx = self.device.rx_bytes()?;
            let rx_bytes = ((current_rx - self.rx_bytes) as f64 / update_interval) as u64;
            self.rx_bytes = current_rx;

            if let Some(ref mut rx) = self.output_rx {
                *rx = format_speed(
                    rx_bytes,
                    self.speed_digits,
                    &self.speed_min_unit.to_string(),
                    self.use_bits,
                );
            };

            if let Some(ref mut graph_rx) = self.graph_rx {
                self.rx_buff.remove(0);
                self.rx_buff.push(rx_bytes);
                *graph_rx = format_vec_to_bar_graph(&self.rx_buff, None, None);
            }
        }
        Ok(())
    }
}

impl Block for Net {
    fn update(&mut self) -> Result<Option<Update>> {
        self.update_device();

        // skip updating if device is not up.
        let exists = self.device.exists()?;
        let is_up = self.device.is_up()?;
        if !exists || !is_up {
            self.active = false;
            self.network.set_text(" ×".to_string());
            if let Some(ref mut tx) = self.output_tx {
                *tx = "×".to_string();
            };
            if let Some(ref mut rx) = self.output_rx {
                *rx = "×".to_string();
            };

            return Ok(Some(self.update_interval.into()));
        }

        self.active = true;
        self.network.set_text("".to_string());

        // Update SSID and IP address every 30s and the bitrate every 10s
        let now = Instant::now();
        if now.duration_since(self.last_update).as_secs() % 10 == 0 {
            self.update_bitrate()?;
        }
        if now.duration_since(self.last_update).as_secs() > 30 {
            self.update_ssid()?;
            self.update_signal_strength()?;
            self.update_ip_addr()?;
            self.last_update = now;
        }

        self.update_tx_rx()?;

        let empty_string = "".to_string();
        let s_up = format!(
            "{} {}",
            self.config
                .icons
                .get("net_up")
                .cloned()
                .unwrap_or_else(|| "".to_string()),
            self.output_tx.as_ref().unwrap_or(&empty_string)
        );
        let s_dn = format!(
            "{} {}",
            self.config
                .icons
                .get("net_down")
                .cloned()
                .unwrap_or_else(|| "".to_string()),
            self.output_rx.as_ref().unwrap_or(&empty_string)
        );

        let values = map!(
            "{ssid}" => self.ssid.as_ref().unwrap_or(&empty_string),
            "{signal_strength}" => self.signal_strength.as_ref().unwrap_or(&empty_string),
            "{signal_strength_bar}" => self.signal_strength_bar.as_ref().unwrap_or(&empty_string),
            "{bitrate}" =>  self.bitrate.as_ref().unwrap_or(&empty_string),
            "{ip}" =>  self.ip_addr.as_ref().unwrap_or(&empty_string),
            "{ipv6}" =>  self.ipv6_addr.as_ref().unwrap_or(&empty_string),
            "{speed_up}" =>  &s_up,
            "{speed_down}" => &s_dn,
            "{graph_up}" =>  self.graph_tx.as_ref().unwrap_or(&empty_string),
            "{graph_down}" =>  self.graph_rx.as_ref().unwrap_or(&empty_string)
        );

        self.output
            .set_text(self.format.render_static_str(&values)?);

        Ok(Some(self.update_interval.into()))
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        if self.active {
            vec![&self.network, &self.output]
        } else if !self.hide_inactive || !self.hide_missing {
            vec![&self.network]
        } else {
            vec![]
        }
    }

    fn click(&mut self, e: &I3BarEvent) -> Result<()> {
        if let Some(ref name) = e.name {
            if name.as_str() == self.id {
                if let MouseButton::Left = e.button {
                    if let Some(ref cmd) = self.on_click {
                        spawn_child_async("sh", &["-c", cmd])
                            .block_error("net", "could not spawn child")?;
                    }
                }
            }
        }

        Ok(())
    }

    fn id(&self) -> &str {
        &self.id
    }
}

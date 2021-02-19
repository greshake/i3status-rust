use std::ffi::OsStr;
use std::fmt;
use std::fs::{read_to_string, OpenOptions};
use std::io::{prelude::*, ErrorKind};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use lazy_static::lazy_static;
use regex::bytes::Regex;
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::SharedConfig;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::input::{I3BarEvent, MouseButton};
use crate::scheduler::Task;
use crate::util::{
    escape_pango_text, format_number, format_percent_bar, format_vec_to_bar_graph, FormatTemplate,
};
use crate::widgets::{text::TextWidget, I3BarWidget, Spacing};

lazy_static! {
    static ref DEFAULT_DEV_REGEX: Regex = Regex::new("default.*dev (\\w*).*").unwrap();
    static ref WHITESPACE_REGEX: Regex = Regex::new("\\s+").unwrap();
    static ref ETHTOOL_SPEED_REGEX: Regex = Regex::new("Speed: (\\d+\\w\\w/s)").unwrap();
    static ref IW_SSID_REGEX: Regex = Regex::new("SSID: (.*)").unwrap();
    static ref WPA_SSID_REGEX: Regex = Regex::new("ssid=([[:alnum:]]+)").unwrap();
    static ref IWCTL_SSID_REGEX: Regex = Regex::new("Connected network\\s+([[:alnum:]]+)").unwrap();
    static ref IW_BITRATE_REGEX: Regex =
        Regex::new("tx bitrate: (\\d+(?:\\.?\\d+) [[:alpha:]]+/s)").unwrap();
    static ref IW_SIGNAL_REGEX: Regex = Regex::new("signal: (-?\\d+) dBm").unwrap();
}

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
            Command::new("ip")
                .args(&["route", "show", "default"])
                .output()
                .ok()
                .and_then(|o| {
                    let mut captures = DEFAULT_DEV_REGEX.captures_iter(&o.stdout);
                    if let Some(cap) = captures.next() {
                        cap.get(1).map(|x| x.as_bytes().to_vec())
                    } else {
                        None
                    }
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

    /// Queries the wireless SSID of this device, if it is connected to one.
    pub fn ssid(&self) -> Result<Option<String>> {
        if self.is_up()? && self.wireless {
            get_ssid(self)
        } else {
            Ok(None)
        }
    }

    fn absolute_signal_strength(&self) -> Result<Option<i32>> {
        if !self.is_up()? || !self.wireless {
            return Ok(None);
        }

        let iw_output = Command::new("iw")
            .args(&["dev", &self.device, "link"])
            .output()
            .block_error("net", "Failed to execute signal strength query.")?
            .stdout;

        if let Some(raw) = IW_SIGNAL_REGEX
            .captures_iter(&iw_output)
            .next()
            .and_then(|x| x.get(1))
        {
            String::from_utf8(raw.as_bytes().to_vec())
                .block_error("net", "Non-UTF8 signal strength")
                .and_then(|s| {
                    s.parse::<i32>()
                        .block_error("net", "Non numerical signal strength.")
                })
                .map(Some)
        } else {
            Ok(None)
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
        let output = Command::new("ip")
            .args(&["-json", "-family", "inet", "address", "show", &self.device])
            .output()
            .block_error("net", "Failed to execute IP address query.")
            .and_then(|raw_output| {
                String::from_utf8(raw_output.stdout)
                    .block_error("net", "Response contained non-UTF8 characters.")
            })?;

        let ip_devs: Vec<IpDev> =
            serde_json::from_str(&output).block_error("net", "Failed to parse JSON response")?;

        if ip_devs.is_empty() {
            return Ok(Some("".to_string()));
        }

        let ip = ip_devs
            .iter()
            .filter(|dev| dev.addr_info.is_some())
            .flat_map(|dev| &dev.addr_info)
            .flatten()
            .filter_map(|addr| addr.local.clone())
            .next();

        Ok(match ip {
            Some(addr) => Some(addr),
            _ => Some("".to_string()),
        })
    }

    /// Queries the inet IPv6 of this device (using `ip`).
    pub fn ipv6_addr(&self) -> Result<Option<String>> {
        if !self.is_up()? {
            return Ok(None);
        }
        let output = Command::new("ip")
            .args(&["-json", "-family", "inet6", "address", "show", &self.device])
            .output()
            .block_error("net", "Failed to execute IP address query.")
            .and_then(|raw_output| {
                String::from_utf8(raw_output.stdout)
                    .block_error("net", "Response contained non-UTF8 characters.")
            })?;

        let ip_devs: Vec<IpDev> =
            serde_json::from_str(&output).block_error("net", "Failed to parse JSON response")?;

        if ip_devs.is_empty() {
            return Ok(Some("".to_string()));
        }

        let ip = ip_devs
            .iter()
            .filter(|dev| dev.addr_info.is_some())
            .flat_map(|dev| &dev.addr_info)
            .flatten()
            .filter_map(|addr| addr.local.clone())
            .next();

        Ok(match ip {
            Some(addr) => Some(addr),
            _ => Some("".to_string()),
        })
    }

    /// Queries the bitrate of this device
    pub fn bitrate(&self) -> Result<Option<String>> {
        let up = self.is_up()?;
        // Doesn't really make sense to crash the bar here
        if !up {
            return Ok(None);
        }
        if self.wireless {
            let bitrate_output = Command::new("iw")
                .args(&["dev", &self.device, "link"])
                .output()
                .block_error("net", "Failed to execute bitrate query with iw.")?
                .stdout;

            if let Some(rate) = IW_BITRATE_REGEX
                .captures_iter(&bitrate_output)
                .next()
                .and_then(|x| x.get(1))
            {
                String::from_utf8(rate.as_bytes().to_vec())
                    .block_error("net", "Non-UTF8 bitrate")
                    .map(Some)
            } else {
                Ok(None)
            }
        } else {
            let output = Command::new("ethtool")
                .arg(&self.device)
                .output()
                .block_error("net", "Failed to execute bitrate query with ethtool")?
                .stdout;
            if let Some(rate) = ETHTOOL_SPEED_REGEX.captures_iter(&output).next() {
                let rate = rate
                    .get(1)
                    .block_error("net", "Invalid ethtool output: no speed")?;
                String::from_utf8(rate.as_bytes().to_vec())
                    .block_error("net", "Non-UTF8 bitrate")
                    .map(Some)
            } else {
                Ok(None)
            }
        }
    }
}

pub struct Net {
    id: usize,
    format: FormatTemplate,
    format_alt: Option<FormatTemplate>,
    is_clicked: bool,
    output: TextWidget,
    ssid: Option<String>,
    max_ssid_width: usize,
    signal_strength: Option<String>,
    signal_strength_bar: Option<String>,
    ip_addr: Option<String>,
    ipv6_addr: Option<String>,
    bitrate: Option<String>,
    output_tx: String,
    output_rx: String,
    graph_tx: String,
    graph_rx: String,
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
    exists: bool,
    hide_inactive: bool,
    hide_missing: bool,
    last_update: Instant,
    shared_config: SharedConfig,
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

    #[serde(default = "NetConfig::default_format_alt")]
    pub format_alt: Option<String>,

    /// Which interface in /sys/class/net/ to read from.
    pub device: Option<String>,

    /// Max SSID width, in characters.
    #[serde(default = "NetConfig::default_max_ssid_width")]
    pub max_ssid_width: usize,

    /// Whether to hide networks that are down/inactive completely.
    #[serde(default = "NetConfig::default_hide_inactive")]
    pub hide_inactive: bool,

    /// Whether to hide networks that are missing.
    #[serde(default = "NetConfig::default_hide_missing")]
    pub hide_missing: bool,

    /// Whether to show speeds in bits or bytes per second.
    #[serde(default = "NetConfig::default_use_bits")]
    pub use_bits: bool,

    /// Number of digits to show for throughput indiciators.
    #[serde(default = "NetConfig::default_speed_digits")]
    pub speed_digits: usize,

    /// Minimum unit to display for throughput indicators.
    #[serde(default = "NetConfig::default_speed_min_unit")]
    pub speed_min_unit: Unit,
}

impl NetConfig {
    fn default_interval() -> Duration {
        Duration::from_secs(1)
    }

    fn default_format() -> String {
        "{speed_up} {speed_down}".to_owned()
    }

    fn default_format_alt() -> Option<String> {
        None
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

    fn default_use_bits() -> bool {
        false
    }

    fn default_speed_min_unit() -> Unit {
        Unit::K
    }

    fn default_speed_digits() -> usize {
        3
    }
}

impl ConfigBlock for Net {
    type Config = NetConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        shared_config: SharedConfig,
        _tx_update_request: Sender<Task>,
    ) -> Result<Self> {
        let default_device = match NetworkDevice::default_device() {
            Some(ref s) if !s.is_empty() => s.to_string(),
            _ => "lo".to_string(),
        };
        let device = match block_config.device.clone() {
            Some(d) => NetworkDevice::from_device(d),
            _ => NetworkDevice::from_device(default_device),
        };
        let init_rx_bytes = device.rx_bytes().unwrap_or(0);
        let init_tx_bytes = device.tx_bytes().unwrap_or(0);
        let wireless = device.is_wireless();
        let vpn = device.is_vpn();

        let format_alt = if let Some(f) = block_config.format_alt {
            Some(
                FormatTemplate::from_string(&f)
                    .block_error("net", "Invalid format_alt specified")?,
            )
        } else {
            None
        };

        Ok(Net {
            id,
            update_interval: block_config.interval,
            format: FormatTemplate::from_string(&block_config.format)
                .block_error("net", "Invalid format specified")?,
            format_alt,
            is_clicked: false,
            output: TextWidget::new(id, shared_config.clone())
                .with_icon(if wireless {
                    "net_wireless"
                } else if vpn {
                    "net_vpn"
                } else if device.device == "lo" {
                    "net_loopback"
                } else {
                    "net_wired"
                })
                .with_text("")
                .with_spacing(Spacing::Inline),
            use_bits: block_config.use_bits,
            speed_min_unit: block_config.speed_min_unit,
            speed_digits: block_config.speed_digits,
            ssid: None,
            max_ssid_width: block_config.max_ssid_width,
            signal_strength: if wireless && block_config.format.contains("{signal_strength}") {
                Some(0.to_string())
            } else {
                None
            },
            signal_strength_bar: if wireless
                && block_config.format.contains("{signal_strength_bar}")
            {
                Some("".to_string())
            } else {
                None
            },
            // TODO: a better way to deal with this?
            bitrate: if block_config.format.contains("{bitrate}") {
                Some("".to_string())
            } else {
                None
            },
            ip_addr: if block_config.format.contains("{ip}") {
                Some("".to_string())
            } else {
                None
            },
            ipv6_addr: if block_config.format.contains("{ipv6}") {
                Some("".to_string())
            } else {
                None
            },
            output_tx: String::new(),
            output_rx: String::new(),
            graph_tx: String::new(),
            graph_rx: String::new(),
            device,
            auto_device: block_config.device.is_none(),
            rx_buff: vec![0; 10],
            tx_buff: vec![0; 10],
            rx_bytes: init_rx_bytes,
            tx_bytes: init_tx_bytes,
            active: true,
            exists: true,
            hide_inactive: block_config.hide_inactive,
            hide_missing: block_config.hide_missing,
            last_update: Instant::now() - Duration::from_secs(30),
            shared_config,
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
            let dev = match NetworkDevice::default_device() {
                Some(ref s) if !s.is_empty() => s.to_string(),
                _ => "lo".to_string(),
            };

            if self.device.device() != dev {
                self.device = NetworkDevice::from_device(dev);
                self.output.set_icon(if self.device.is_wireless() {
                    "net_wireless"
                } else if self.device.is_vpn() {
                    "net_vpn"
                } else if self.device.device == "lo" {
                    "net_loopback"
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
        if let Some(s) = self.device.ssid()? {
            let mut truncated = s;
            truncated.truncate(self.max_ssid_width);
            // SSID names can contain chars that need escaping
            self.ssid = Some(escape_pango_text(truncated));
        } else {
            self.ssid = None;
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
            + (self.update_interval.subsec_nanos() as f64 / 1_000_000_000.0);

        // Update the throughput/graph widgets if they are enabled
        let current_tx = self.device.tx_bytes()?;
        let diff = current_tx.checked_sub(self.tx_bytes).unwrap_or(0);
        let tx_bytes = (diff as f64 / update_interval) as u64;
        self.tx_bytes = current_tx;

        self.output_tx = format_number(
            if self.use_bits {
                tx_bytes * 8
            } else {
                tx_bytes
            } as f64,
            self.speed_digits,
            &self.speed_min_unit.to_string(),
            if self.use_bits { "b" } else { "B" },
        );

        self.tx_buff.remove(0);
        self.tx_buff.push(tx_bytes);
        self.graph_tx = format_vec_to_bar_graph(&self.tx_buff, None, None);

        let current_rx = self.device.rx_bytes()?;
        let diff = current_rx.checked_sub(self.rx_bytes).unwrap_or(0);
        let rx_bytes = (diff as f64 / update_interval) as u64;
        self.rx_bytes = current_rx;

        self.output_rx = format_number(
            if self.use_bits {
                rx_bytes * 8
            } else {
                rx_bytes
            } as f64,
            self.speed_digits,
            &self.speed_min_unit.to_string(),
            if self.use_bits { "b" } else { "B" },
        );

        self.rx_buff.remove(0);
        self.rx_buff.push(rx_bytes);
        self.graph_rx = format_vec_to_bar_graph(&self.rx_buff, None, None);

        Ok(())
    }
}

impl Block for Net {
    fn update(&mut self) -> Result<Option<Update>> {
        self.update_device();

        // skip updating if device is not up.
        self.exists = self.device.exists()?;
        self.active = self.exists && self.device.is_up()?;
        if !self.active {
            self.output.set_text("×".to_string());
            return Ok(Some(self.update_interval.into()));
        }

        // Update SSID and IP address every 30s and the bitrate every 10s
        let now = Instant::now();
        if now.duration_since(self.last_update).as_secs() % 10 == 0 {
            self.update_bitrate()?;
        }

        let waiting_for_ip = match self.ip_addr.as_deref() {
            None => false,
            Some("") => true,
            Some(_) => false,
        };

        let waiting_for_ipv6 = match self.ipv6_addr.as_deref() {
            None => false,
            Some("") => true,
            Some(_) => false,
        };

        if (now.duration_since(self.last_update).as_secs() > 30)
            || waiting_for_ip
            || waiting_for_ipv6
        {
            self.update_ssid()?;
            self.update_signal_strength()?;
            self.update_ip_addr()?;
            self.last_update = now;
        }

        self.update_tx_rx()?;

        let mut s_up = self.shared_config.get_icon("net_up").unwrap_or_default();
        s_up.push_str(&self.output_tx);
        let mut s_dn = self.shared_config.get_icon("net_down").unwrap_or_default();
        s_dn.push_str(&self.output_rx);

        let empty_string = "".to_string();
        let na_string = "N/A".to_string();

        let values = map!(
            "{ssid}" => self.ssid.as_ref().unwrap_or(&na_string),
            "{signal_strength}" => self.signal_strength.as_ref().unwrap_or(&na_string),
            "{signal_strength_bar}" => self.signal_strength_bar.as_ref().unwrap_or(&empty_string),
            "{bitrate}" => self.bitrate.as_ref().unwrap_or(&empty_string),
            "{ip}" => self.ip_addr.as_ref().unwrap_or(&empty_string),
            "{ipv6}" => self.ipv6_addr.as_ref().unwrap_or(&empty_string),
            "{speed_up}" => &s_up,
            "{speed_down}" => &s_dn,
            "{graph_up}" => &self.graph_tx,
            "{graph_down}" => &self.graph_rx
        );

        let format = if self.is_clicked {
            // calling unwrap() here is OK, because is_clicked could be set only if format_alt is some
            self.format_alt.as_ref().unwrap()
        } else {
            &self.format
        };

        self.output.set_text(format.render_static_str(&values)?);

        Ok(Some(self.update_interval.into()))
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        if (!self.active && self.hide_inactive) || (!self.exists && self.hide_missing) {
            vec![]
        } else {
            vec![&self.output]
        }
    }

    fn click(&mut self, event: &I3BarEvent) -> Result<()> {
        if event.matches_id(self.id)
            && event.button == MouseButton::Left
            && self.format_alt.is_some()
        {
            self.is_clicked = !self.is_clicked;
            self.update()?;
        }
        Ok(())
    }

    fn id(&self) -> usize {
        self.id
    }
}

#[derive(Deserialize)]
struct IpDev {
    addr_info: Option<Vec<IpAddrInfo>>,
}

#[derive(Deserialize)]
struct IpAddrInfo {
    local: Option<String>,
}

fn get_ssid(dev: &NetworkDevice) -> Result<Option<String>> {
    if let Some(res) = get_iw_ssid(dev)? {
        return Ok(Some(res));
    }

    if let Some(res) = get_wpa_ssid(dev)? {
        return Ok(Some(res));
    }

    if let Some(res) = get_nmcli_ssid(dev)? {
        return Ok(Some(res));
    }

    if let Some(res) = get_iwctl_ssid(dev)? {
        return Ok(Some(res));
    }

    Ok(None)
}

#[inline]
/// Attempt to get the SSID the given device is connected to from iw.
/// Returns Err if:
///     - `iw` is not a valid command
///     - failed to spawn an `iw` command
///     - `iw` failed to produce a valid UTF-8 SSID
/// Returns Ok(None) if `iw` failed to produce a SSID.
fn get_iw_ssid(dev: &NetworkDevice) -> Result<Option<String>> {
    let raw = exec_ssid_cmd("iw", &["dev", &dev.device, "link"])?;

    if raw.is_none() {
        return Ok(None);
    }

    let raw = raw.unwrap();
    let result = raw
        .stdout
        .split(|c| *c == b'\n')
        .filter_map(|x| IW_SSID_REGEX.captures_iter(x).next())
        .filter_map(|x| x.get(1))
        .next();

    maybe_ssid_convert(result.map(|x| x.as_bytes()))
}

#[inline]
/// Attempt to get the SSID the given device is connected to from wpa_cli.
/// Returns Err if:
///     - `wpa_cli` is not a valid command
///     - failed to spawn a `wpa_cli` command
///     - `wpa_cli` failed to produce a valid UTF-8 SSID
/// Returns Ok(None) if `wpa_cli` failed to produce a SSID.
fn get_wpa_ssid(dev: &NetworkDevice) -> Result<Option<String>> {
    let raw = exec_ssid_cmd("wpa_cli", &["status", "-i", &dev.device])?;

    if raw.is_none() {
        return Ok(None);
    }

    let raw = raw.unwrap();
    let result = raw
        .stdout
        .split(|c| *c == b'\n')
        .filter_map(|x| WPA_SSID_REGEX.find(x))
        .next();

    maybe_ssid_convert(result.map(|x| x.as_bytes()))
}

#[inline]
/// Attempt to get the SSID the given device is connected to from nmcli.
/// Returns Err if:
///     - `nmcli` is not a valid command
///     - failed to spawn a `nmcli` command
///     - `nmcli` failed to produce a valid UTF-8 SSID
/// Returns Ok(None) if `nmcli` failed to produce a SSID.
fn get_nmcli_ssid(dev: &NetworkDevice) -> Result<Option<String>> {
    let raw = exec_ssid_cmd(
        "nmcli",
        &["-g", "general.connection", "device", "show", &dev.device],
    )?;

    if raw.is_none() {
        return Ok(None);
    }

    let raw = raw.unwrap();
    let result = raw.stdout.split(|c| *c == b'\n').next();

    maybe_ssid_convert(result)
}

#[inline]
/// Attempt to get the SSID the given device is connected to from iwctl.
/// Returns Err if
///     - `iwctl` is not a valid command
///     - failed to spawn a `iwctl` command
///     - `iwctl` failed to produce a valid UTF-8 SSID
/// Returns Ok(None) if `iwctl` failed to produce a SSID.
fn get_iwctl_ssid(dev: &NetworkDevice) -> Result<Option<String>> {
    let raw = exec_ssid_cmd("iwctl", &["station", &dev.device, "show"])?;

    if raw.is_none() {
        return Ok(None);
    }

    let raw = raw.unwrap();
    let result = raw
        .stdout
        .split(|c| *c == b'\n')
        .filter_map(|x| IWCTL_SSID_REGEX.captures_iter(x).next())
        .filter_map(|x| x.get(1))
        .next();

    maybe_ssid_convert(result.map(|x| x.as_bytes()))
}

#[inline]
fn exec_ssid_cmd<S, I, L>(cmd: S, args: I) -> Result<Option<std::process::Output>>
where
    S: AsRef<std::ffi::OsStr>,
    I: IntoIterator<Item = L>,
    L: AsRef<OsStr>,
{
    let raw = Command::new(&cmd).args(args).output();

    if let Err(ref err) = raw {
        if err.kind() == ErrorKind::NotFound {
            return Ok(None);
        }
    }

    raw.map(Some).block_error(
        "net",
        &format!(
            "Failed to execute SSID query using {}",
            cmd.as_ref().to_string_lossy()
        ),
    )
}

#[inline]
fn maybe_ssid_convert(raw: Option<&[u8]>) -> Result<Option<String>> {
    if let Some(raw_ssid) = raw {
        String::from_utf8(decode_escaped_unicode(raw_ssid))
            .block_error("net", "Non-UTF8 SSID")
            .map(Some)
    } else {
        Ok(None)
    }
}

fn decode_escaped_unicode(raw: &[u8]) -> Vec<u8> {
    let mut result: Vec<u8> = Vec::new();

    let mut idx = 0;
    while idx < raw.len() {
        if raw[idx] == b'\\' {
            idx += 2; // skip "\x"

            let hex = std::str::from_utf8(&raw[idx..idx + 2]).unwrap();
            result.extend(Some(u8::from_str_radix(hex, 16).unwrap()));
            idx += 2;
        } else {
            result.extend(Some(&raw[idx]));
            idx += 1;
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use crate::blocks::net::maybe_ssid_convert;

    #[test]
    fn test_ssid_decode_escaped_unicode() {
        assert_eq!(
            maybe_ssid_convert(Some(r"\xc4\x85\xc5\xbeuolas".as_bytes())).unwrap(),
            Some("ąžuolas".to_string())
        );
    }

    #[test]
    fn test_ssid_decode_escaped_emoji() {
        assert_eq!(
            maybe_ssid_convert(Some(r"\xf0\x9f\x8c\xb3oak".as_bytes())).unwrap(),
            Some("🌳oak".to_string())
        );
    }

    #[test]
    fn test_ssid_decode_legit_backslash() {
        assert_eq!(
            maybe_ssid_convert(Some(r"\x5cx backslash".as_bytes())).unwrap(),
            Some(r"\x backslash".to_string())
        );
    }

    #[test]
    fn test_ssid_decode_surrounded_by_spaces() {
        assert_eq!(
            maybe_ssid_convert(Some(r"\x20surrounded by spaces\x20".as_bytes())).unwrap(),
            Some(r" surrounded by spaces ".to_string())
        );
    }
}

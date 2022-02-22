use std::fmt;
use std::fs::{read_to_string, OpenOptions};
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use lazy_static::lazy_static;
use regex::bytes::{Captures, Regex};
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::SharedConfig;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::formatting::value::Value;
use crate::formatting::FormatTemplate;
use crate::protocol::i3bar_event::{I3BarEvent, MouseButton};
use crate::scheduler::Task;
use crate::util::{escape_pango_text, format_vec_to_bar_graph};
use crate::widgets::{text::TextWidget, I3BarWidget, Spacing};

lazy_static! {
    static ref DEFAULT_DEV_REGEX: Regex = Regex::new("default.*dev (\\w*).*").unwrap();
    static ref WHITESPACE_REGEX: Regex = Regex::new("\\s+").unwrap();
    static ref ETHTOOL_SPEED_REGEX: Regex = Regex::new("Speed: (\\d+\\w\\w/s)").unwrap();
    static ref IW_BITRATE_REGEX: Regex =
        Regex::new("tx bitrate: (\\d+(?:\\.?\\d+) [[:alpha:]]+/s)").unwrap();
}

#[derive(Debug)]
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
    pub fn wifi_info(&self) -> Result<(Option<String>, Option<f64>, Option<i64>)> {
        if !self.is_up()? || !self.wireless {
            return Ok((None, None, None));
        }

        let mut sock = neli_wifi::Socket::connect()
            .block_error("net", "nl80211: failed to connect to the socket")?;

        let interfaces = sock
            .get_interfaces_info()
            .block_error("net", "nl80211: failed to get interfaces' information")?;

        for interface in interfaces {
            if let Some(index) = &interface.index {
                if let Ok(ap) = sock.get_station_info(index) {
                    // SSID is `None` when not connected
                    if let (Some(ssid), Some(device)) = (interface.ssid, interface.name) {
                        let device = String::from_utf8_lossy(&device);
                        let device = device.trim_matches(char::from(0));
                        if device != self.device {
                            continue;
                        }

                        let ssid = Some(escape_pango_text(&decode_escaped_unicode(&ssid)));
                        let freq = interface.frequency.map(|f| f as f64 * 1e6);
                        let signal = ap
                            .signal
                            .or_else(|| {
                                sock.get_bss_info(index)
                                    .ok()
                                    .and_then(|bss| bss.signal)
                                    .map(|s| (s / 100) as i8)
                            })
                            .map(signal_percents);

                        return Ok((ssid, freq, signal));
                    }
                }
            }
        }

        Ok((None, None, None))
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
    output: TextWidget,
    ip_addr: Option<String>,
    ipv6_addr: Option<String>,
    bitrate: Option<String>,
    speed_up: f64,
    speed_down: f64,
    graph_tx: String,
    graph_rx: String,
    update_interval: Duration,
    device: NetworkDevice,
    auto_device: bool,
    tx_buff: Vec<f64>,
    rx_buff: Vec<f64>,
    tx_bytes: u64,
    rx_bytes: u64,
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

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct NetConfig {
    /// Update interval in seconds
    #[serde(deserialize_with = "deserialize_duration")]
    pub interval: Duration,

    pub format: FormatTemplate,

    pub format_alt: Option<FormatTemplate>,

    /// Which interface in /sys/class/net/ to read from.
    pub device: Option<String>,

    /// Whether to hide networks that are down/inactive completely.
    pub hide_inactive: bool,

    /// Whether to hide networks that are missing.
    pub hide_missing: bool,
}

impl Default for NetConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(1),
            format: FormatTemplate::default(),
            format_alt: None,
            device: None,
            hide_inactive: false,
            hide_missing: false,
        }
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

        let format = block_config
            .format
            .with_default("{speed_down;K}{speed_up;K}")?;
        let format_alt = block_config.format_alt;

        Ok(Net {
            id,
            update_interval: block_config.interval,
            output: TextWidget::new(id, 0, shared_config.clone())
                .with_icon(if wireless {
                    "net_wireless"
                } else if vpn {
                    "net_vpn"
                } else if device.device == "lo" {
                    "net_loopback"
                } else {
                    "net_wired"
                })?
                .with_text("")
                .with_spacing(Spacing::Inline),
            // TODO: a better way to deal with this?
            bitrate: (format.contains("bitrate")
                || format_alt
                    .as_ref()
                    .map(|f| f.contains("bitrate"))
                    .unwrap_or(false))
            .then(String::new),
            ip_addr: (format.contains("ip")
                || format_alt
                    .as_ref()
                    .map(|f| f.contains("ip"))
                    .unwrap_or(false))
            .then(String::new),
            ipv6_addr: (format.contains("ipv6")
                || format_alt
                    .as_ref()
                    .map(|f| f.contains("ipv6"))
                    .unwrap_or(false))
            .then(String::new),
            speed_up: 0.0,
            speed_down: 0.0,
            graph_tx: String::new(),
            graph_rx: String::new(),
            device,
            auto_device: block_config.device.is_none(),
            rx_buff: vec![0.; 10],
            tx_buff: vec![0.; 10],
            rx_bytes: init_rx_bytes,
            tx_bytes: init_tx_bytes,
            active: true,
            exists: true,
            hide_inactive: block_config.hide_inactive,
            hide_missing: block_config.hide_missing,
            last_update: Instant::now() - Duration::from_secs(30),
            shared_config,
            format,
            format_alt,
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
    fn update_bitrate(&mut self) -> Result<()> {
        if let Some(ref mut bitrate_string) = self.bitrate {
            let bitrate = self.device.bitrate()?;
            if let Some(b) = bitrate {
                *bitrate_string = b;
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
        let diff = current_tx.saturating_sub(self.tx_bytes);
        let tx_bytes = (diff as f64 / update_interval) as u64;
        self.tx_bytes = current_tx;

        self.speed_up = tx_bytes as f64;

        self.tx_buff.remove(0);
        self.tx_buff.push(tx_bytes as f64);
        self.graph_tx = format_vec_to_bar_graph(&self.tx_buff, None, None);

        let current_rx = self.device.rx_bytes()?;
        let diff = current_rx.saturating_sub(self.rx_bytes);
        let rx_bytes = (diff as f64 / update_interval) as u64;
        self.rx_bytes = current_rx;

        self.speed_down = rx_bytes as f64;

        self.rx_buff.remove(0);
        self.rx_buff.push(rx_bytes as f64);
        self.graph_rx = format_vec_to_bar_graph(&self.rx_buff, None, None);

        Ok(())
    }
}

impl Block for Net {
    fn update(&mut self) -> Result<Option<Update>> {
        // Update device
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
                })?;
            }
        }

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
            self.update_ip_addr()?;
            self.last_update = now;
        }

        self.update_tx_rx()?;

        let (ssid, freq, signal) = self.device.wifi_info()?;

        let empty_string = "".to_string();
        let na_string = "N/A".to_string();

        let values = map!(
            "ssid" => Value::from_string(ssid.unwrap_or(na_string)),
            "signal_strength" => Value::from_integer(signal.unwrap_or(0)).percents(),
            "frequency" => Value::from_float(freq.unwrap_or(0.)).hertz(),
            "bitrate" => Value::from_string(self.bitrate.clone().unwrap_or_else(|| empty_string.clone())), // TODO: not a String?
            "ip" => Value::from_string(self.ip_addr.clone().unwrap_or_else(|| empty_string.clone())),
            "ipv6" => Value::from_string(self.ipv6_addr.clone().unwrap_or(empty_string)),
            "speed_up" => Value::from_float(self.speed_up).bytes().icon(self.shared_config.get_icon("net_up")?),
            "speed_down" => Value::from_float(self.speed_down).bytes().icon(self.shared_config.get_icon("net_down")?),
            "graph_up" => Value::from_string(self.graph_tx.clone()),
            "graph_down" => Value::from_string(self.graph_rx.clone()),
        );

        self.output.set_texts(self.format.render(&values)?);

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
        if event.button == MouseButton::Left {
            if let Some(ref mut format) = self.format_alt {
                std::mem::swap(format, &mut self.format);
            }
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

fn decode_escaped_unicode(raw: &[u8]) -> String {
    // Match escape sequences like \x2a or \x0D
    let re = Regex::new(r"\\x([0-9A-Fa-f]{2})").unwrap();

    let result = re.replace_all(raw, |caps: &Captures| {
        let hex = std::str::from_utf8(&caps[1]).unwrap();
        let byte = u8::from_str_radix(hex, 16).unwrap();
        [byte; 1]
    });

    String::from_utf8_lossy(&result).to_string()
}

fn signal_percents(raw: i8) -> i64 {
    let raw = raw as f64;

    let perfect = -20.;
    let worst = -85.;
    let d = perfect - worst;

    // https://github.com/torvalds/linux/blob/9ff9b0d392ea08090cd1780fb196f36dbb586529/drivers/net/wireless/intel/ipw2x00/ipw2200.c#L4322-L4334
    let percents = 100. - (perfect - raw) * (15. * d + 62. * (perfect - raw)) / (d * d);

    (percents as i64).clamp(0, 100)
}

#[cfg(test)]
mod tests {
    use crate::blocks::net::decode_escaped_unicode;

    #[test]
    fn test_ssid_decode_escaped_unicode() {
        assert_eq!(
            decode_escaped_unicode(r"\xc4\x85\xc5\xbeuolas".as_bytes()),
            "ąžuolas".to_string()
        );
    }

    #[test]
    fn test_ssid_decode_escaped_emoji() {
        assert_eq!(
            decode_escaped_unicode(r"\xf0\x9f\x8c\xb3oak".as_bytes()),
            "🌳oak".to_string()
        );
    }

    #[test]
    fn test_ssid_decode_legit_backslash() {
        assert_eq!(
            decode_escaped_unicode(r"\x5cx backslash".as_bytes()),
            r"\x backslash".to_string()
        );
    }

    #[test]
    fn test_ssid_decode_surrounded_by_spaces() {
        assert_eq!(
            decode_escaped_unicode(r"\x20surrounded by spaces\x20".as_bytes()),
            r" surrounded by spaces ".to_string()
        );
    }

    #[test]
    fn test_ssid_decode_noescape_path() {
        assert_eq!(
            decode_escaped_unicode(r"C:\Program Files(x86)\Custom\Utilities\Tool.exe".as_bytes()),
            r"C:\Program Files(x86)\Custom\Utilities\Tool.exe".to_string()
        );
    }

    #[test]
    fn test_ssid_decode_noescape_invalid() {
        assert_eq!(
            decode_escaped_unicode(r"\xp0".as_bytes()),
            r"\xp0".to_string()
        );
    }
}

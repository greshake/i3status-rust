use neli::attr::Attribute as _;
use neli::consts::{nl::*, rtnl::*, socket::*};
use neli::nl::{NlPayload, Nlmsghdr};
use neli::rtnl::{Ifaddrmsg, Ifinfomsg, Rtmsg};
use neli::socket::{NlSocketHandle, tokio::NlSocket};
use neli::types::RtBuffer;

use regex::Regex;

use libc::c_uchar;

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::ops;
use std::path::Path;

use crate::errors::*;
use crate::util;

// From `linux/rtnetlink.h`
const RT_SCOPE_HOST: c_uchar = 254;

#[derive(Debug)]
pub struct NetDevice {
    pub iface: Interface,
    pub wifi_info: Option<WifiInfo>,
    pub ip: Option<Ipv4Addr>,
    pub ipv6: Option<Ipv6Addr>,
    pub icon: &'static str,
    pub tun_wg_ppp: bool,
    pub nameservers: Vec<IpAddr>,
}

#[derive(Debug, Default)]
pub struct WifiInfo {
    pub ssid: Option<String>,
    pub signal: Option<f64>,
    pub frequency: Option<f64>,
    pub bitrate: Option<f64>,
}

impl NetDevice {
    pub async fn new(iface_re: Option<&Regex>) -> Result<Option<Self>> {
        let mut sock = NlSocket::new(
            NlSocketHandle::connect(NlFamily::Route, None, &[]).error("Socket error")?,
        )
        .error("Socket error")?;

        let mut ifaces = get_interfaces(&mut sock, iface_re)
            .await
            .map_err(BoxErrorWrapper)
            .error("Failed to fetch interfaces")?;
        if ifaces.is_empty() {
            return Ok(None);
        }

        let default_iface = get_default_interface(&mut sock)
            .await
            .map_err(BoxErrorWrapper)
            .error("Failed to get default interface")?;

        let iface_position = ifaces
            .iter()
            .position(|i| i.index == default_iface)
            .or_else(|| ifaces.iter().position(|i| i.operstate == Operstate::Up))
            .unwrap_or(0);

        let iface = ifaces.swap_remove(iface_position);
        let wifi_info = WifiInfo::new(iface.index).await?;
        let ip = ipv4(&mut sock, iface.index).await?;
        let ipv6 = ipv6(&mut sock, iface.index).await?;
        let nameservers = read_nameservers()
            .await
            .error("Failed to read nameservers")?;

        // TODO: use netlink for the these too
        // I don't believe that this should ever change, so set it now:
        let path = Path::new("/sys/class/net").join(&iface.name);
        let tun = iface.name.starts_with("tun")
            || iface.name.starts_with("tap")
            || path.join("tun_flags").exists();
        let (wg, ppp) = util::read_file(path.join("uevent"))
            .await
            .map_or((false, false), |c| {
                (c.contains("wireguard"), c.contains("ppp"))
            });

        let icon = if wifi_info.is_some() {
            "net_wireless"
        } else if tun || wg || ppp {
            "net_vpn"
        } else if iface.name == "lo" {
            "net_loopback"
        } else {
            "net_wired"
        };

        Ok(Some(Self {
            iface,
            wifi_info,
            ip,
            ipv6,
            icon,
            tun_wg_ppp: tun | wg | ppp,
            nameservers,
        }))
    }

    pub fn is_up(&self) -> bool {
        self.tun_wg_ppp
            || self.iface.operstate == Operstate::Up
            || (self.iface.operstate == Operstate::Unknown
                && (self.ip.is_some() || self.ipv6.is_some()))
    }

    pub fn ssid(&self) -> Option<String> {
        self.wifi_info.as_ref()?.ssid.clone()
    }

    pub fn frequency(&self) -> Option<f64> {
        self.wifi_info.as_ref()?.frequency
    }

    pub fn bitrate(&self) -> Option<f64> {
        self.wifi_info.as_ref()?.bitrate
    }

    pub fn signal(&self) -> Option<f64> {
        self.wifi_info.as_ref()?.signal
    }
}

impl WifiInfo {
    async fn new(if_index: i32) -> Result<Option<Self>> {
        /// <https://github.com/torvalds/linux/blob/9ff9b0d392ea08090cd1780fb196f36dbb586529/drivers/net/wireless/intel/ipw2x00/ipw2200.c#L4322-L4334>
        fn signal_percents(raw: f64) -> f64 {
            const MAX_LEVEL: f64 = -20.;
            const MIN_LEVEL: f64 = -85.;
            const DIFF: f64 = MAX_LEVEL - MIN_LEVEL;
            (100. - (MAX_LEVEL - raw) * (15. * DIFF + 62. * (MAX_LEVEL - raw)) / (DIFF * DIFF))
                .clamp(0., 100.)
        }

        fn ssid_from_bss_info_elements(mut bytes: &[u8]) -> Option<String> {
            while bytes.len() > 2 && bytes[0] != 0 {
                bytes = &bytes[(bytes[1] as usize + 2)..];
            }

            if bytes.len() < 2 || bytes.len() < bytes[1] as usize + 2 {
                return None;
            };

            let ssid_len = bytes[1] as usize;
            let raw_ssid = &bytes[2..][..ssid_len];

            Some(String::from_utf8_lossy(raw_ssid).into_owned())
        }

        // Ignore connection error because `nl80211` might not be enabled on the system.
        let Ok(mut socket) = neli_wifi::AsyncSocket::connect() else {
            return Ok(None);
        };

        let interfaces = socket
            .get_interfaces_info()
            .await
            .error("Failed to get nl80211 interfaces")?;

        for interface in interfaces {
            if let Some(index) = interface.index {
                if index != if_index {
                    continue;
                }

                let Ok(ap) = socket.get_station_info(index).await else {
                    continue;
                };

                // TODO: are there any situations when there is more than one station?
                let Some(ap) = ap.into_iter().next() else {
                    continue;
                };

                let bss = socket
                    .get_bss_info(index)
                    .await
                    .unwrap_or_default()
                    .into_iter()
                    .find(|bss| bss.status == Some(1));

                let raw_signal = match ap.signal {
                    Some(signal) => Some(signal),
                    None => bss
                        .as_ref()
                        .and_then(|bss| bss.signal)
                        .map(|s| (s / 100) as i8),
                };

                let ssid = interface
                    .ssid
                    .as_deref()
                    .map(|ssid| String::from_utf8_lossy(ssid).into_owned())
                    .or_else(|| {
                        bss.as_ref()
                            .and_then(|bss| bss.information_elements.as_deref())
                            .and_then(ssid_from_bss_info_elements)
                    });

                return Ok(Some(Self {
                    ssid,
                    frequency: interface.frequency.map(|f| f as f64 * 1e6),
                    signal: raw_signal.map(|s| signal_percents(s as f64)),
                    bitrate: ap.tx_bitrate.map(|b| b as f64 * 1e5), // 100kbit/s -> bit/s
                }));
            }
        }
        Ok(None)
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct InterfaceStats {
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}

impl ops::Sub for InterfaceStats {
    type Output = Self;

    fn sub(mut self, rhs: Self) -> Self::Output {
        self.rx_bytes = self.rx_bytes.saturating_sub(rhs.rx_bytes);
        self.tx_bytes = self.tx_bytes.saturating_sub(rhs.tx_bytes);
        self
    }
}

impl InterfaceStats {
    fn from_stats64(stats: &[u8]) -> Self {
        // stats looks something like that:
        //
        // #[repr(C)]
        // struct RtnlLinkStats64 {
        //     rx_packets: u64,
        //     tx_packets: u64,
        //     rx_bytes: u64,
        //     tx_bytes: u64,
        //     // the rest is omitted
        // }
        assert!(stats.len() >= 8 * 4);
        Self {
            rx_bytes: u64::from_ne_bytes(stats[16..24].try_into().unwrap()),
            tx_bytes: u64::from_ne_bytes(stats[24..32].try_into().unwrap()),
        }
    }
}

#[derive(Debug)]
pub struct Interface {
    pub index: i32,
    pub operstate: Operstate,
    pub name: String,
    pub stats: Option<InterfaceStats>,
}

macro_rules! recv_until_done {
    ($sock:ident, $payload:ident: $payload_type:ty => $($code:tt)*) => {
        let mut buf = Vec::new();
        'recv: loop {
            let msgs = $sock.recv::<u16, $payload_type>(&mut buf).await?;
            for msg in msgs {
                if msg.nl_type == libc::NLMSG_DONE as u16 {
                    break 'recv;
                }
                if let NlPayload::Payload($payload) = msg.nl_payload {
                    $($code)*
                }
            }
        }
    };
}

async fn get_interfaces(
    sock: &mut NlSocket,
    filter: Option<&Regex>,
) -> Result<Vec<Interface>, Box<dyn StdError + Send + Sync + 'static>> {
    sock.send(&Nlmsghdr::new(
        None,
        Rtm::Getlink,
        NlmFFlags::new(&[NlmF::Dump, NlmF::Request]),
        None,
        None,
        NlPayload::Payload(Ifinfomsg::new(
            RtAddrFamily::Unspecified,
            Arphrd::None,
            0,
            IffFlags::empty(),
            IffFlags::empty(),
            RtBuffer::new(),
        )),
    ))
    .await?;

    let mut interfaces = Vec::new();

    recv_until_done!(sock, msg: Ifinfomsg => {
        let mut name = None;
        let mut stats = None;
        let mut operstate = Operstate::Unknown;
        for attr in msg.rtattrs.iter() {
            match attr.rta_type {
                Ifla::Ifname => name = Some(attr.get_payload_as_with_len()?),
                Ifla::Stats64 => stats = Some(InterfaceStats::from_stats64(attr.payload().as_ref())),
                Ifla::Operstate => operstate = attr.get_payload_as::<u8>()?.into(),
                _ => (),
            }
        }
        let name: String = name.unwrap();
        if filter.is_none_or(|f| f.is_match(&name)) {
            interfaces.push(Interface {
                index: msg.ifi_index,
                operstate,
                name,
                stats,
            });
        }
    });

    Ok(interfaces)
}

async fn get_default_interface(
    sock: &mut NlSocket,
) -> Result<i32, Box<dyn StdError + Send + Sync + 'static>> {
    sock.send(&Nlmsghdr::new(
        None,
        Rtm::Getroute,
        NlmFFlags::new(&[NlmF::Request, NlmF::Dump]),
        None,
        None,
        NlPayload::Payload(Rtmsg {
            rtm_family: RtAddrFamily::Inet,
            rtm_dst_len: 0,
            rtm_src_len: 0,
            rtm_tos: 0,
            rtm_table: RtTable::Unspec,
            rtm_protocol: Rtprot::Unspec,
            rtm_scope: RtScope::Universe,
            rtm_type: Rtn::Unspec,
            rtm_flags: RtmFFlags::empty(),
            rtattrs: RtBuffer::new(),
        }),
    ))
    .await?;

    let mut best_index = 0;
    let mut best_metric = u32::MAX;

    recv_until_done!(sock, msg: Rtmsg => {
        if msg.rtm_type != Rtn::Unicast {
            continue;
        }
        // Only check default routes (rtm_dst_len == 0)
        if msg.rtm_dst_len != 0 {
            continue;
        }

        let mut index = None;
        let mut metric = 0u32;
        for attr in msg.rtattrs.iter() {
            match attr.rta_type {
                Rta::Oif => index = Some(attr.get_payload_as::<i32>()?),
                Rta::Priority => metric = attr.get_payload_as::<u32>()?,
                _ => (),
            }
        }
        if let Some(i) = index {
            if metric < best_metric {
                best_metric = metric;
                best_index = i;
            }
        }
    });

    Ok(best_index)
}

async fn ip_payload<const BYTES: usize>(
    sock: &mut NlSocket,
    ifa_family: RtAddrFamily,
    ifa_index: i32,
) -> Result<Option<[u8; BYTES]>, Box<dyn StdError + Send + Sync + 'static>> {
    sock.send(&Nlmsghdr::new(
        None,
        Rtm::Getaddr,
        NlmFFlags::new(&[NlmF::Dump, NlmF::Request]),
        None,
        None,
        NlPayload::Payload(Ifaddrmsg {
            ifa_family,
            ifa_prefixlen: 0,
            ifa_flags: IfaFFlags::empty(),
            ifa_scope: 0,
            ifa_index: 0,
            rtattrs: RtBuffer::new(),
        }),
    ))
    .await?;

    let mut payload = None;

    recv_until_done!(sock, msg: Ifaddrmsg => {
        if msg.ifa_index != ifa_index || msg.ifa_scope >= RT_SCOPE_HOST || payload.is_some() {
            continue;
        }

        let attr_handle = msg.rtattrs.get_attr_handle();

        let Some(attr) = attr_handle.get_attribute(Ifa::Local)
            .or_else(|| attr_handle.get_attribute(Ifa::Address))
        else { continue };

        if let Ok(p) = attr.rta_payload.as_ref().try_into() {
            payload = Some(p);
        }
    });

    Ok(payload)
}

async fn ipv4(sock: &mut NlSocket, ifa_index: i32) -> Result<Option<Ipv4Addr>> {
    Ok(ip_payload(sock, RtAddrFamily::Inet, ifa_index)
        .await
        .map_err(BoxErrorWrapper)
        .error("Failed to get IP address")?
        .map(Ipv4Addr::from))
}

async fn ipv6(sock: &mut NlSocket, ifa_index: i32) -> Result<Option<Ipv6Addr>> {
    Ok(ip_payload(sock, RtAddrFamily::Inet6, ifa_index)
        .await
        .map_err(BoxErrorWrapper)
        .error("Failed to get IPv6 address")?
        .map(Ipv6Addr::from))
}

async fn read_nameservers() -> Result<Vec<IpAddr>> {
    let file = util::read_file("/etc/resolv.conf")
        .await
        .error("Failed to read /etc/resolv.conf")?;
    let mut nameservers = Vec::new();

    for line in file.lines() {
        let mut line_parts = line.split_whitespace();
        if line_parts.next() == Some("nameserver") {
            if let Some(mut ip) = line_parts.next() {
                // TODO: use the zone id somehow?
                if let Some((without_zone_id, _zone_id)) = ip.split_once('%') {
                    ip = without_zone_id;
                }
                nameservers.push(ip.parse().error("Unable to parse ip")?);
            }
        }
    }

    Ok(nameservers)
}

// Source: https://www.kernel.org/doc/Documentation/networking/operstates.txt
#[derive(Debug, PartialEq, Eq)]
pub enum Operstate {
    /// Interface is in unknown state, neither driver nor userspace has set
    /// operational state. Interface must be considered for user data as
    /// setting operational state has not been implemented in every driver.
    Unknown,
    /// Unused in current kernel (notpresent interfaces normally disappear),
    /// just a numerical placeholder.
    Notpresent,
    /// Interface is unable to transfer data on L1, f.e. ethernet is not
    /// plugged or interface is ADMIN down.
    Down,
    /// Interfaces stacked on an interface that is IF_OPER_DOWN show this
    /// state (f.e. VLAN).
    Lowerlayerdown,
    /// Unused in current kernel.
    Testing,
    /// Interface is L1 up, but waiting for an external event, f.e. for a
    /// protocol to establish. (802.1X)
    Dormant,
    /// Interface is operational up and can be used.
    Up,
}

impl From<u8> for Operstate {
    fn from(value: u8) -> Self {
        match value {
            1 => Self::Notpresent,
            2 => Self::Down,
            3 => Self::Lowerlayerdown,
            4 => Self::Testing,
            5 => Self::Dormant,
            6 => Self::Up,
            _ => Self::Unknown,
        }
    }
}

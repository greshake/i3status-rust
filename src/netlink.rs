use neli::attr::Attribute;
use neli::consts::{nl::*, rtnl::*, socket::*};
use neli::nl::{NlPayload, Nlmsghdr};
use neli::rtnl::{Ifaddrmsg, Ifinfomsg, Rtmsg};
use neli::socket::{tokio::NlSocket, NlSocketHandle};
use neli::types::RtBuffer;

use regex::Regex;

use std::net::{Ipv4Addr, Ipv6Addr};
use std::ops;
use std::path::Path;

use crate::errors::*;
use crate::util;

// Source: https://www.kernel.org/doc/Documentation/networking/operstates.txt
//
/// Interface is in unknown state, neither driver nor userspace has set
/// operational state. Interface must be considered for user data as
/// setting operational state has not been implemented in every driver.
#[allow(dead_code)]
const IF_OPER_UNKNOWN: u8 = 0;
/// Unused in current kernel (notpresent interfaces normally disappear),
/// just a numerical placeholder.
#[allow(dead_code)]
const IF_OPER_NOTPRESENT: u8 = 1;
/// Interface is unable to transfer data on L1, f.e. ethernet is not
/// plugged or interface is ADMIN down.
#[allow(dead_code)]
const IF_OPER_DOWN: u8 = 2;
/// Interfaces stacked on an interface that is IF_OPER_DOWN show this
/// state (f.e. VLAN).
#[allow(dead_code)]
const IF_OPER_LOWERLAYERDOWN: u8 = 3;
/// Unused in current kernel.
#[allow(dead_code)]
const IF_OPER_TESTING: u8 = 4;
/// Interface is L1 up, but waiting for an external event, f.e. for a
/// protocol to establish. (802.1X)
#[allow(dead_code)]
const IF_OPER_DORMANT: u8 = 5;
/// Interface is operational up and can be used.
const IF_OPER_UP: u8 = 6;

#[derive(Debug)]
pub struct NetDevice {
    pub iface: Interface,
    pub wifi_info: Option<WifiInfo>,
    pub ip: Option<Ipv4Addr>,
    pub ipv6: Option<Ipv6Addr>,
    pub icon: &'static str,
    pub tun_wg_ppp: bool,
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

        let ifaces = get_interfaces(&mut sock)
            .await
            .map_err(BoxErrorWrapper)
            .error("Failed to fetch interfaces")?;

        let iface = match iface_re {
            Some(re) => ifaces.into_iter().find(|i| re.is_match(&i.name)),
            None => {
                let default_iface = get_default_interface(&mut sock)
                    .await
                    .map_err(BoxErrorWrapper)
                    .error("Failed to get default interface")?;
                ifaces.into_iter().find(|i| i.index == default_iface)
            }
        };

        let iface = match iface {
            Some(iface) => iface,
            None => return Ok(None),
        };

        let wifi_info = WifiInfo::new(iface.index).await?;
        let ip = ipv4(&mut sock, iface.index).await?;
        let ipv6 = ipv6(&mut sock, iface.index).await?;

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
        }))
    }

    pub fn is_up(&self) -> bool {
        self.iface.is_up || self.tun_wg_ppp
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

        let mut socket =
            neli_wifi::AsyncSocket::connect().error("Failed to open nl80211 socket")?;
        let interfaces = socket
            .get_interfaces_info()
            .await
            .error("Failed to get nl80211 interfaces")?;
        for interface in interfaces {
            if let Some(index) = interface.index {
                if index != if_index {
                    continue;
                }
                if let Ok(ap) = socket.get_station_info(index).await {
                    let raw_signal = match ap.signal {
                        Some(signal) => Some(signal),
                        None => socket
                            .get_bss_info(index)
                            .await
                            .ok()
                            .and_then(|bss| bss.signal)
                            .map(|s| (s / 100) as i8),
                    };
                    return Ok(Some(Self {
                        ssid: interface
                            .ssid
                            .map(String::from_utf8)
                            .transpose()
                            .error("SSID is not valid UTF8")?, // ssid: Some(String::from_utf8(ssid).error("SSID is not valid UTF8")?),
                        frequency: interface.frequency.map(|f| f as f64 * 1e6),
                        signal: raw_signal.map(|s| signal_percents(s as f64)),
                        bitrate: ap.tx_bitrate.map(|b| b as f64 * 1e5), // 100kbit/s -> bit/s
                    }));
                }
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
        let stats = stats.as_ptr() as *const u64;
        Self {
            rx_bytes: unsafe { stats.add(2).read_unaligned() },
            tx_bytes: unsafe { stats.add(3).read_unaligned() },
        }
    }
}

#[derive(Debug)]
pub struct Interface {
    pub index: i32,
    pub is_up: bool,
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
        let mut is_up = false;
        for attr in msg.rtattrs.iter() {
            match attr.rta_type {
                Ifla::Ifname => name = Some(attr.get_payload_as_with_len()?),
                Ifla::Stats64 => stats = Some(InterfaceStats::from_stats64(attr.payload().as_ref())),
                Ifla::Operstate => is_up = attr.get_payload_as::<u8>()? == IF_OPER_UP,
                _ => (),
            }
        }
        interfaces.push(Interface {
            index: msg.ifi_index,
            is_up,
            name: name.unwrap(),
            stats,
        });
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

    let mut default_index = 0;

    recv_until_done!(sock, msg: Rtmsg => {
        if msg.rtm_type != Rtn::Unicast {
            continue;
        }
        let mut index = None;
        let mut is_default = false;
        for attr in msg.rtattrs.iter() {
            match attr.rta_type {
                Rta::Oif => index = Some(attr.get_payload_as::<i32>()?),
                Rta::Gateway => is_default = true,
                _ => (),
            }
        }
        if is_default && default_index == 0 {
            default_index = index.unwrap();
        }
    });

    Ok(default_index)
}

async fn ip_payload(
    sock: &mut NlSocket,
    ifa_family: RtAddrFamily,
    ifa_index: i32,
) -> Result<Option<neli::types::Buffer>, Box<dyn StdError + Send + Sync + 'static>> {
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
        if msg.ifa_index != ifa_index {
            continue;
        }
        if let Some(rtattr) = msg.rtattrs.into_iter().find(|a| a.rta_type == Ifa::Address) {
            payload = Some(rtattr.rta_payload);
        }
    });

    Ok(payload)
}

async fn ipv4(sock: &mut NlSocket, ifa_index: i32) -> Result<Option<Ipv4Addr>> {
    match ip_payload(sock, RtAddrFamily::Inet, ifa_index)
        .await
        .map_err(BoxErrorWrapper)
        .error("Failed to get IP address")?
    {
        None => Ok(None),
        Some(payload) => {
            let payload: &[u8; 4] = payload.as_ref().try_into().unwrap();
            Ok(Some(Ipv4Addr::from(*payload)))
        }
    }
}

async fn ipv6(sock: &mut NlSocket, ifa_index: i32) -> Result<Option<Ipv6Addr>> {
    match ip_payload(sock, RtAddrFamily::Inet6, ifa_index)
        .await
        .map_err(BoxErrorWrapper)
        .error("Failed to get IPv6 address")?
    {
        None => Ok(None),
        Some(payload) => {
            let payload: &[u8; 16] = payload.as_ref().try_into().unwrap();
            Ok(Some(Ipv6Addr::from(*payload)))
        }
    }
}

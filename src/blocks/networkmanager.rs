use std::fmt;
use std::net::Ipv4Addr;

use zbus::dbus_proxy;

use super::prelude::*;

#[derive(Deserialize, Debug, Default)]
#[serde(deny_unknown_fields, default)]
struct NetworkManagerConfig {
    /// Whether to only show the primary connection, or all active connections.
    primary_only: bool,
    /// AP formatter
    ap_format: FormatConfig,
    /// Device formatter.
    device_format: FormatConfig,
    /// Connection formatter.
    connection_format: FormatConfig,
    /// Interface name regex patterns to ignore.
    interface_name_exclude: Vec<String>,
    /// Interface name regex patterns to include.
    interface_name_include: Vec<String>,
}

pub async fn run(config: toml::Value, mut api: CommonApi) -> Result<()> {
    let config = NetworkManagerConfig::deserialize(config).config_error()?;

    /*
    let dbus_conn = Connection::get_private(BusType::System)
        .block_error("networkmanager", "failed to establish D-Bus connection")?;
    let manager = ConnectionManager::new();

    thread::Builder::new()
        .name("networkmanager".into())
        .spawn(move || {
            let c = Connection::get_private(BusType::System).unwrap();

            c.add_match(
                "type='signal',\
                    path='/org/freedesktop/NetworkManager',\
                    interface='org.freedesktop.DBus.Properties',\
                    member='PropertiesChanged'",
            )
            .unwrap();
            c.add_match(
                "type='signal',\
                    path_namespace='/org/freedesktop/NetworkManager/ActiveConnection',\
                    interface='org.freedesktop.DBus.Properties',\
                    member='PropertiesChanged'",
            )
            .unwrap();

            loop {
                let timeout = 300_000;

                for event in c.iter(timeout) {
                    match event {
                        ConnectionItem::Nothing => (),
                        _ => send
                            .send(Task {
                                id,
                                update_time: Instant::now(),
                            })
                            .unwrap(),
                    }
                }
            }
        })
        .unwrap();

    fn compile_regexps(patterns: Vec<String>) -> result::Result<Vec<Regex>, regex::Error> {
        patterns.iter().map(|p| Regex::new(p)).collect()
    }

    let x = Ok(NetworkManager {
        id,
        indicator: TextWidget::new(id, 0, shared_config.clone()),
        output: Vec::new(),
        dbus_conn,
        manager,
        primary_only: block_config.primary_only,
        ap_format: block_config.ap_format.with_default("{ssid}")?,
        device_format: block_config
            .device_format
            .with_default("{icon}{ap} {ips}")?,
        connection_format: block_config.connection_format.with_default("{devices}")?,
        interface_name_exclude_regexps: compile_regexps(block_config.interface_name_exclude)
            .block_error("networkmanager", "failed to parse exclude patterns")?,
        interface_name_include_regexps: compile_regexps(block_config.interface_name_include)
            .block_error("networkmanager", "failed to parse include patterns")?,
        shared_config,
    });
    */

    loop {}
}

#[derive(Debug)]
enum NetworkState {
    Unknown,
    Asleep,
    Disconnected,
    Disconnecting,
    Connecting,
    ConnectedLocal,
    ConnectedSite,
    ConnectedGlobal,
}

impl From<u32> for NetworkState {
    fn from(id: u32) -> Self {
        // https://developer.gnome.org/NetworkManager/stable/nm-dbus-types.html#NMState
        match id {
            10 => NetworkState::Asleep,
            20 => NetworkState::Disconnected,
            30 => NetworkState::Disconnecting,
            40 => NetworkState::Connecting,
            50 => NetworkState::ConnectedLocal,
            60 => NetworkState::ConnectedSite,
            70 => NetworkState::ConnectedGlobal,
            _ => NetworkState::Unknown,
        }
    }
}

#[derive(Debug)]
enum ActiveConnectionState {
    Unknown,
    Activating,
    Activated,
    Deactivating,
    Deactivated,
}

impl From<u32> for ActiveConnectionState {
    fn from(id: u32) -> Self {
        // https://developer.gnome.org/NetworkManager/stable/nm-dbus-types.html#NMActiveConnectionState
        match id {
            1 => ActiveConnectionState::Activating,
            2 => ActiveConnectionState::Activated,
            3 => ActiveConnectionState::Deactivating,
            4 => ActiveConnectionState::Deactivated,
            _ => ActiveConnectionState::Unknown,
        }
    }
}

impl ActiveConnectionState {
    fn to_state(&self, good: State) -> State {
        match self {
            ActiveConnectionState::Activated => good,
            ActiveConnectionState::Activating => State::Warning,
            ActiveConnectionState::Deactivating => State::Warning,
            ActiveConnectionState::Deactivated => State::Critical,
            ActiveConnectionState::Unknown => State::Critical,
        }
    }
}

#[derive(Debug)]
enum DeviceType {
    Unknown,
    Ethernet,
    Wifi,
    Modem,
    Bridge,
    Tun,
    Wireguard,
}

impl From<u32> for DeviceType {
    fn from(id: u32) -> Self {
        // https://developer.gnome.org/NetworkManager/stable/nm-dbus-types.html#NMDeviceType
        match id {
            1 => DeviceType::Ethernet,
            2 => DeviceType::Wifi,
            8 => DeviceType::Modem,
            13 => DeviceType::Bridge,
            16 => DeviceType::Tun,
            29 => DeviceType::Wireguard,
            _ => DeviceType::Unknown,
        }
    }
}

impl DeviceType {
    fn to_icon_name(&self) -> Option<&'static str> {
        match self {
            DeviceType::Ethernet => Some("net_wired"),
            DeviceType::Wifi => Some("net_wireless"),
            DeviceType::Modem => Some("net_modem"),
            DeviceType::Bridge => Some("net_bridge"),
            DeviceType::Tun => Some("net_bridge"),
            DeviceType::Wireguard => Some("net_vpn"),
            _ => None,
        }
    }
}

#[derive(Debug)]
struct Ipv4Address {
    address: Ipv4Addr,
    prefix: u32,
    _gateway: Ipv4Addr,
}

trait ByteOrderSwap {
    fn swap(&self) -> Self;
}

impl ByteOrderSwap for u32 {
    fn swap(&self) -> u32 {
        ((self & 0x0000_00FF) << 24)
            | ((self & 0x0000_FF00) << 8)
            | ((self & 0x00FF_0000) >> 8)
            | ((self & 0xFF00_0000) >> 24)
    }
}

/*
impl<'a> From<Array<'a, u32, Iter<'a>>> for Ipv4Address {
    fn from(s: Array<'a, u32, Iter<'a>>) -> Ipv4Address {
        let mut i = s;
        Ipv4Address {
            address: Ipv4Addr::from(i.next().unwrap().swap()),
            prefix: i.next().unwrap(),
            gateway: Ipv4Addr::from(i.next().unwrap().swap()),
        }
    }
}
*/

impl fmt::Display for Ipv4Address {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}/{}", self.address, self.prefix)
    }
}

struct ConnectionManager;

impl ConnectionManager {
    async fn state(&self, c: &zbus::Connection) -> Result<NetworkState> {
        let state = NetworkManagerProxy::new(c)
            .await
            .unwrap()
            .state()
            .await
            .error("Failed to retrieve state")?;
        Ok(state.into())
    }

    async fn primary_connection(&self, c: &zbus::Connection) -> Result<NmConnection> {
        let m = Self::get_property(c, "PrimaryConnection")
            .block_error("networkmanager", "Failed to retrieve primary connection")?;

        let primary_connection: Variant<Path> = m
            .get1()
            .block_error("networkmanager", "Failed to read primary connection")?;

        if primary_connection.0.to_string() == "/" {
            return Err(BlockError(
                "networkmanager".to_string(),
                "No primary connection".to_string(),
            ));
        }

        Ok(NmConnection {
            path: primary_connection.0.clone(),
        })
    }

    pub fn active_connections(&self, c: &Connection) -> Result<Vec<NmConnection>> {
        let m = Self::get_property(c, "ActiveConnections")
            .block_error("networkmanager", "Failed to retrieve active connections")?;

        let active_connections: Variant<Array<Path, Iter>> = m
            .get1()
            .block_error("networkmanager", "Failed to read active connections")?;

        Ok(active_connections
            .0
            .map(|x| NmConnection { path: x })
            .collect())
    }
}

// #[derive(Clone)]
// struct NmConnection<'a> {
//     path: Path<'a>,
// }

// impl<'a> NmConnection<'a> {
//     fn state(&self, c: &Connection) -> Result<ActiveConnectionState> {
//         let m = ConnectionManager::get(
//             c,
//             self.path.clone(),
//             "org.freedesktop.NetworkManager.Connection.Active",
//             "State",
//         )
//         .block_error("networkmanager", "Failed to retrieve connection state")?;

//         let state: Variant<u32> = m
//             .get1()
//             .block_error("networkmanager", "Failed to read connection state")?;
//         Ok(ActiveConnectionState::from(state.0))
//     }

//     fn vpn(&self, c: &Connection) -> Result<bool> {
//         let m = ConnectionManager::get(
//             c,
//             self.path.clone(),
//             "org.freedesktop.NetworkManager.Connection.Active",
//             "Vpn",
//         )
//         .block_error("networkmanager", "Failed to retrieve connection vpn flag")?;

//         let vpn: Variant<bool> = m
//             .get1()
//             .block_error("networkmanager", "Failed to read connection vpn flag")?;
//         Ok(vpn.0)
//     }

//     fn id(&self, c: &Connection) -> Result<String> {
//         let m = ConnectionManager::get(
//             c,
//             self.path.clone(),
//             "org.freedesktop.NetworkManager.Connection.Active",
//             "Id",
//         )
//         .block_error("networkmanager", "Failed to retrieve connection ID")?;

//         let id: Variant<String> = m
//             .get1()
//             .block_error("networkmanager", "Failed to read Id")?;
//         Ok(id.0)
//     }

//     fn devices(&self, c: &Connection) -> Result<Vec<NmDevice>> {
//         let m = ConnectionManager::get(
//             c,
//             self.path.clone(),
//             "org.freedesktop.NetworkManager.Connection.Active",
//             "Devices",
//         )
//         .block_error("networkmanager", "Failed to retrieve connection device")?;

//         let devices: Variant<Array<Path, Iter>> = m
//             .get1()
//             .block_error("networkmanager", "Failed to read devices")?;
//         Ok(devices.0.map(|x| NmDevice { path: x }).collect())
//     }
// }

// #[derive(Clone)]
// struct NmDevice<'a> {
//     path: Path<'a>,
// }

// impl<'a> NmDevice<'a> {
//     fn device_type(&self, c: &Connection) -> Result<DeviceType> {
//         let m = ConnectionManager::get(
//             c,
//             self.path.clone(),
//             "org.freedesktop.NetworkManager.Device",
//             "DeviceType",
//         )
//         .block_error("networkmanager", "Failed to retrieve device type")?;

//         let device_type: Variant<u32> = m
//             .get1()
//             .block_error("networkmanager", "Failed to read device type")?;
//         Ok(DeviceType::from(device_type.0))
//     }

//     fn interface_name(&self, c: &Connection) -> Result<String> {
//         let m = ConnectionManager::get(
//             c,
//             self.path.clone(),
//             "org.freedesktop.NetworkManager.Device",
//             "Interface",
//         )
//         .block_error("networkmanager", "Failed to retrieve device interface name")?;

//         let interface_name: Variant<String> = m
//             .get1()
//             .block_error("networkmanager", "Failed to read interface name")?;

//         Ok(interface_name.0)
//     }

//     fn ip4config(&self, c: &Connection) -> Result<NmIp4Config> {
//         let m = ConnectionManager::get(
//             c,
//             self.path.clone(),
//             "org.freedesktop.NetworkManager.Device",
//             "Ip4Config",
//         )
//         .block_error("networkmanager", "Failed to retrieve device ip4config")?;

//         let ip4config: Variant<Path> = m
//             .get1()
//             .block_error("networkmanager", "Failed to read ip4config")?;
//         Ok(NmIp4Config { path: ip4config.0 })
//     }

//     fn active_access_point(&self, c: &Connection) -> Result<NmAccessPoint> {
//         let m = ConnectionManager::get(
//             c,
//             self.path.clone(),
//             "org.freedesktop.NetworkManager.Device.Wireless",
//             "ActiveAccessPoint",
//         )
//         .block_error(
//             "networkmanager",
//             "Failed to retrieve device active access point",
//         )?;

//         let active_ap: Variant<Path> = m
//             .get1()
//             .block_error("networkmanager", "Failed to read active access point")?;
//         Ok(NmAccessPoint { path: active_ap.0 })
//     }
// }

// #[derive(Clone)]
// struct NmAccessPoint<'a> {
//     path: Path<'a>,
// }

// impl<'a> NmAccessPoint<'a> {
//     fn ssid(&self, c: &Connection) -> Result<String> {
//         let m = ConnectionManager::get(
//             c,
//             self.path.clone(),
//             "org.freedesktop.NetworkManager.AccessPoint",
//             "Ssid",
//         )
//         .block_error("networkmanager", "Failed to retrieve SSID")?;

//         let ssid: Variant<Array<u8, Iter>> = m
//             .get1()
//             .block_error("networkmanager", "Failed to read ssid")?;
//         Ok(std::str::from_utf8(&ssid.0.collect::<Vec<u8>>())
//             .block_error("networkmanager", "Failed to parse ssid")?
//             .to_string())
//     }

//     fn strength(&self, c: &Connection) -> Result<u8> {
//         let m = ConnectionManager::get(
//             c,
//             self.path.clone(),
//             "org.freedesktop.NetworkManager.AccessPoint",
//             "Strength",
//         )
//         .block_error("networkmanager", "Failed to retrieve strength")?;

//         let strength: Variant<u8> = m
//             .get1()
//             .block_error("networkmanager", "Failed to read strength")?;
//         Ok(strength.0)
//     }

//     fn frequency(&self, c: &Connection) -> Result<u32> {
//         let m = ConnectionManager::get(
//             c,
//             self.path.clone(),
//             "org.freedesktop.NetworkManager.AccessPoint",
//             "Frequency",
//         )
//         .block_error("networkmanager", "Failed to retrieve frequency")?;

//         let frequency: Variant<u32> = m
//             .get1()
//             .block_error("networkmanager", "Failed to read frequency")?;
//         Ok(frequency.0)
//     }
// }

// #[derive(Clone)]
// struct NmIp4Config<'a> {
//     path: Path<'a>,
// }

// impl<'a> NmIp4Config<'a> {
//     fn addresses(&self, c: &Connection) -> Result<Vec<Ipv4Address>> {
//         let m = ConnectionManager::get(
//             c,
//             self.path.clone(),
//             "org.freedesktop.NetworkManager.IP4Config",
//             "Addresses",
//         )
//         .block_error("networkmanager", "Failed to retrieve addresses")?;

//         let addresses: Variant<Array<Array<u32, Iter>, Iter>> = m
//             .get1()
//             .block_error("networkmanager", "Failed to read addresses")?;
//         Ok(addresses.0.map(Ipv4Address::from).collect())
//     }
// }

/*
impl Block for NetworkManager {
    fn update(&mut self) -> Result<Option<Update>> {
        let state = self.manager.state(&self.dbus_conn);

        self.indicator.set_state(match state {
            Ok(NetworkState::ConnectedGlobal) => State::Good,
            Ok(NetworkState::ConnectedSite) => State::Info,
            Ok(NetworkState::ConnectedLocal) => State::Idle,
            Ok(NetworkState::Connecting) => State::Warning,
            Ok(NetworkState::Disconnecting) => State::Warning,
            _ => State::Critical,
        });
        self.indicator.set_text(match state {
            Ok(NetworkState::Disconnected) => "×".to_string(),
            Ok(NetworkState::Asleep) => "×".to_string(),
            Ok(NetworkState::Unknown) => "E".to_string(),
            _ => String::new(),
        });

        self.output = match state {
            // It would be a waste of time to bother NetworkManager in any of these states
            Ok(NetworkState::Disconnected)
            | Ok(NetworkState::Asleep)
            | Ok(NetworkState::Unknown) => vec![],

            _ => {
                let good_state = match state {
                    Ok(NetworkState::ConnectedGlobal) => State::Good,
                    Ok(NetworkState::ConnectedSite) => State::Info,
                    _ => State::Idle,
                };

                let connections = if self.primary_only {
                    match self.manager.primary_connection(&self.dbus_conn) {
                        Ok(conn) => vec![conn],
                        Err(_) => vec![],
                    }
                } else {
                    // We sort things so that the primary connection comes first
                    let active = self
                        .manager
                        .active_connections(&self.dbus_conn)
                        .unwrap_or_else(|_| Vec::new());
                    match self.manager.primary_connection(&self.dbus_conn) {
                        Ok(conn) => vec![conn.clone()]
                            .into_iter()
                            .chain(active.into_iter().filter(|x| x.path != conn.path))
                            .collect(),
                        Err(_) => active,
                    }
                };

                connections
                    .into_iter()
                    .filter_map(|conn| {
                        // Hide vpn connection(s) since its devices are the devices of its parent connection
                        if let Ok(true) = conn.vpn(&self.dbus_conn) {
                            return None;
                        };

                        // inline spacing for no leading space, because the icon is set in the string
                        let mut widget = TextWidget::new(self.id, 0, self.shared_config.clone())
                            .with_spacing(Spacing::Inline);

                        // Set the state for this connection
                        widget.set_state(if let Ok(conn_state) = conn.state(&self.dbus_conn) {
                            conn_state.to_state(good_state)
                        } else {
                            ActiveConnectionState::Unknown.to_state(good_state)
                        });

                        // Get all devices for this connection
                        let mut devicevec: Vec<String> = Vec::new();
                        if let Ok(devices) = conn.devices(&self.dbus_conn) {
                            'devices: for device in devices {
                                let name = match device.interface_name(&self.dbus_conn) {
                                    Ok(v) => v,
                                    Err(_) => "".to_string(),
                                };

                                // If an interface matches an exclude pattern, ignore it
                                if self
                                    .interface_name_exclude_regexps
                                    .iter()
                                    .any(|regex| regex.is_match(&name))
                                {
                                    continue 'devices;
                                }

                                // If we have at-least one include pattern, make sure
                                // the interface name matches at least one of them
                                if !self.interface_name_include_regexps.is_empty()
                                    && !self
                                        .interface_name_include_regexps
                                        .iter()
                                        .any(|regex| regex.is_match(&name))
                                {
                                    continue 'devices;
                                }

                                let (icon, type_name) =
                                    if let Ok(dev_type) = device.device_type(&self.dbus_conn) {
                                        match dev_type.to_icon_name() {
                                            Some(icon_name) => {
                                                let i = self
                                                    .shared_config
                                                    .get_icon(&icon_name)
                                                    .unwrap_or_default();
                                                (i, format!("{:?}", dev_type).to_string())
                                            }
                                            None => (
                                                self.shared_config
                                                    .get_icon("unknown")
                                                    .unwrap_or_default(),
                                                format!("{:?}", dev_type).to_string(),
                                            ),
                                        }
                                    } else {
                                        // TODO: Communicate the error to the user?
                                        ("".to_string(), "".to_string())
                                    };

                                let ap = if let Ok(ap) = device.active_access_point(&self.dbus_conn)
                                {
                                    let ssid = ap.ssid(&self.dbus_conn).unwrap_or_else(|_| "".to_string());
                                    let strength = ap.strength(&self.dbus_conn).unwrap_or(0);
                                    let freq = match ap.frequency(&self.dbus_conn) {
                                        Ok(v) => v.to_string(),
                                        Err(_) => "0".to_string(),
                                    };

                                    let values = map!(
                                        "ssid" => Value::from_string(escape_pango_text(&ssid)),
                                        "strength" => Value::from_integer(strength as i64).percents(),
                                        "freq" => Value::from_string(freq).percents(),
                                    );
                                    if let Ok(s) = self.ap_format.render(&values) {
                                        s.0
                                    } else {
                                        "[invalid device format string]".to_string()
                                    }
                                } else {
                                    "".to_string()
                                };

                                let mut ips = "×".to_string();
                                if let Ok(ip4config) = device.ip4config(&self.dbus_conn) {
                                    if let Ok(addresses) = ip4config.addresses(&self.dbus_conn) {
                                        if !addresses.is_empty() {
                                            ips = addresses
                                                .into_iter()
                                                .map(|x| x.to_string())
                                                .collect::<Vec<String>>()
                                                .join(",");
                                        }
                                    }
                                }

                                let values = map!(
                                    "icon" => Value::from_string(icon),
                                    "typename" => Value::from_string(type_name),
                                    "ap" => Value::from_string(ap),
                                    "name" => Value::from_string(name.to_string()),
                                    "ips" => Value::from_string(ips),
                                );

                                if let Ok(s) = self.device_format.render(&values) {
                                    devicevec.push(s.0);
                                } else {
                                    devicevec.push("[invalid device format string]".to_string())
                                }
                            }
                        };

                        let id = match conn.id(&self.dbus_conn) {
                            Ok(id) => id,
                            Err(v) => format!("{:?}", v),
                        };

                        let values = map!(
                            "devices" => Value::from_string(devicevec.join(" ")),
                            "id" => Value::from_string(id),
                        );

                        if let Ok(s) = self.connection_format.render(&values) {
                            widget.set_texts(s);
                        } else {
                            widget.set_text("[invalid connection format string]".to_string());
                        }

                        if !devicevec.is_empty() {
                            Some(widget)
                        } else {
                            None
                        }
                    })
                    .collect()
            }
        };

        Ok(None)
    }

    // fn view(&self) -> Vec<&dyn I3BarWidget> {
    //     if self.output.is_empty() {
    //         vec![&self.indicator]
    //     } else {
    //         self.output.iter().map(|x| x as &dyn I3BarWidget).collect()
    //     }
    // }
}
*/

// # DBus interface proxy for: `org.freedesktop.NetworkManager`
//
// This code was generated by `zbus-xmlgen` `2.0.0` from DBus introspection data.
// Source: `11`.
//
// You may prefer to adapt it, instead of using it verbatim.
//
// More information can be found in the
// [Writing a client proxy](https://dbus.pages.freedesktop.org/zbus/client.html)
// section of the zbus documentation.
//
// This DBus object implements
// [standard DBus interfaces](https://dbus.freedesktop.org/doc/dbus-specification.html),
// (`org.freedesktop.DBus.*`) for which the following zbus proxies can be used:
//
// * [`zbus::fdo::PropertiesProxy`]
// * [`zbus::fdo::IntrospectableProxy`]
// * [`zbus::fdo::PeerProxy`]
//
// …consequently `zbus-xmlgen` did not generate code for the above interfaces.

#[dbus_proxy(
    interface = "org.freedesktop.NetworkManager",
    default_path = "/org/freedesktop/NetworkManager",
    default_service = "org.freedesktop.NetworkManager"
)]
trait NetworkManager {
    /// ActiveConnections property
    #[dbus_proxy(property)]
    fn active_connections(&self) -> zbus::Result<Vec<zbus::zvariant::OwnedObjectPath>>;

    /// Devices property
    #[dbus_proxy(property)]
    fn devices(&self) -> zbus::Result<Vec<zbus::zvariant::OwnedObjectPath>>;

    /// PrimaryConnection property
    #[dbus_proxy(property)]
    fn primary_connection(&self) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;

    /// State property
    #[dbus_proxy(property)]
    fn state(&self) -> zbus::Result<u32>;
}

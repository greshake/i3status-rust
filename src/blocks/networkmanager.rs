use std::fmt;
use std::net::Ipv4Addr;

use zbus::dbus_proxy;
use zbus::fdo::DBusProxy;
use zbus::zvariant::OwnedObjectPath;

use super::prelude::*;

// TODO add to icon sets
const CROSS: &str = "×";

#[derive(Deserialize, Debug, Default)]
#[serde(deny_unknown_fields, default)]
struct NetworkManagerConfig {
    format: FormatConfig,
    primary_only: bool,
}

pub async fn run(config: toml::Value, mut api: CommonApi) -> Result<()> {
    let mut events = api.get_events().await?;
    let config = NetworkManagerConfig::deserialize(config).config_error()?;
    api.set_format(config.format.with_default("TODO")?);

    let manager = ConnectionManager::new().await?;
    let mut updates = create_updates_stream().await?;
    let mut cur_connection: Option<usize> = None;

    loop {
        // Using a loop because of the early-returns
        loop {
            let state = manager.state().await?;
            if matches!(
                state,
                NetworkState::Disconnected | NetworkState::Asleep | NetworkState::Unknown
            ) {
                api.set_icon_raw(CROSS.into());
                api.set_state(State::Critical);
                break;
            }

            let connection = if config.primary_only {
                let c = manager.primary_connection().await?;
                cur_connection = c.as_ref().map(|_| 0);
                c
            } else {
                let c_all = manager.active_connections().await?;
                let mut c = Vec::new();
                for c_all in c_all {
                    // Hide vpn connection(s) since its devices are the devices of its parent
                    // connection
                    if !c_all.vpn().await? {
                        c.push(c_all);
                    }
                }
                if !c.is_empty() {
                    if let Some(cur) = &mut cur_connection {
                        *cur = (*cur).min(c.len() - 1);
                        c.into_iter().nth(*cur)
                    } else {
                        c.into_iter().next()
                    }
                } else {
                    None
                }
            };
            let connection = match connection {
                Some(c) => c,
                None => {
                    api.set_icon_raw(CROSS.into());
                    api.set_state(State::Warning);
                    break;
                }
            };
            api.set_state(match connection.state().await? {
                ActiveConnectionState::Activated => State::Idle,
                ActiveConnectionState::Activating => State::Warning,
                ActiveConnectionState::Deactivating => State::Warning,
                ActiveConnectionState::Deactivated => State::Critical,
                ActiveConnectionState::Unknown => State::Critical,
            });

            let devices = connection.devices().await?;
            dbg!(devices);

            break;
        }
        api.flush().await?;

        loop {
            tokio::select! {
                Some(BlockEvent::Click(_click)) = events.recv() => {
                    // TODO
                }
                Some(update) = updates.next() => {
                    // We don't care _what_ the update is.
                    // But we probably should.
                    // TODO: filter the updates.
                    let _ = update.error("Bad update")?;
                    break;
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

// #[derive(Debug)]
// struct Ipv4Address {
//     address: Ipv4Addr,
//     prefix: u32,
//     _gateway: Ipv4Addr,
// }
//
// trait ByteOrderSwap {
//     fn swap(&self) -> Self;
// }
//
// impl ByteOrderSwap for u32 {
//     fn swap(&self) -> u32 {
//         ((self & 0x0000_00FF) << 24)
//             | ((self & 0x0000_FF00) << 8)
//             | ((self & 0x00FF_0000) >> 8)
//             | ((self & 0xFF00_0000) >> 24)
//     }
// }

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

// impl fmt::Display for Ipv4Address {
//     fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
//         write!(f, "{}/{}", self.address, self.prefix)
//     }
// }

/// An abstraction over NetworkManagerProxy
struct ConnectionManager(NetworkManagerProxy<'static>);

impl ConnectionManager {
    async fn new() -> Result<Self> {
        let conn = new_system_dbus_connection().await?;
        Ok(Self(
            NetworkManagerProxy::new(&conn)
                .await
                .error("Failed to create NetworkManagerProxy")?,
        ))
    }

    async fn state(&self) -> Result<NetworkState> {
        self.0
            .state()
            .await
            .error("Failed to retrieve state")
            .map(Into::into)
    }

    async fn primary_connection(&self) -> Result<Option<NmConnection>> {
        let pc = self
            .0
            .primary_connection()
            .await
            .error("Failed to retrieve primary connection")?;
        if pc.as_str() != "/" {
            NmConnection::new(self.0.connection(), pc).await.map(Some)
        } else {
            Ok(None)
        }
    }

    async fn active_connections(&self) -> Result<Vec<NmConnection>> {
        let paths = self
            .0
            .active_connections()
            .await
            .error("Failed to retrieve active connections")?;
        let mut res = Vec::with_capacity(paths.len());
        for path in paths {
            res.push(NmConnection::new(self.0.connection(), path).await?);
        }
        Ok(res)
    }
}

/// An abstraction over NetworkManagerConnectionProxy
#[derive(Debug, Clone)]
struct NmConnection(NetworkManagerConnectionProxy<'static>);

impl NmConnection {
    async fn new(con: &zbus::Connection, path: OwnedObjectPath) -> Result<Self> {
        NetworkManagerConnectionProxy::builder(con)
            .path(path)
            .error("Faled to set path")?
            .build()
            .await
            .error("Failed to create NetworkManagerConnectionProxy")
            .map(Self)
    }

    async fn state(&self) -> Result<ActiveConnectionState> {
        self.0
            .state()
            .await
            .error("Failed to retrieve connection state")
            .map(Into::into)
    }

    async fn vpn(&self) -> Result<bool> {
        self.0
            .vpn()
            .await
            .error("Failed to retrieve connection vpn falg")
    }

    async fn id(&self) -> Result<String> {
        self.0
            .id()
            .await
            .error("Failed to retrieve connection ID")
            .map(Into::into)
    }

    async fn devices(&self) -> Result<Vec<NmDevice>> {
        let paths = self
            .0
            .devices()
            .await
            .error("Failed to retrieve connection device")?;
        let mut res = Vec::with_capacity(paths.len());
        for path in paths {
            res.push(NmDevice::new(self.0.connection(), path).await?);
        }
        Ok(res)
    }
}

#[derive(Debug, Clone)]
struct NmDevice(OwnedObjectPath);

impl NmDevice {
    async fn new(con: &zbus::Connection, path: OwnedObjectPath) -> Result<Self> {
        Ok(Self(path))
    }
}

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

/// Returns a stream of dbus updates. Yes, it will trigger an update even for properties we don't
/// care about, but it's _much_ easier.
async fn create_updates_stream() -> Result<zbus::MessageStream> {
    let conn = new_system_dbus_connection().await?;
    let proxy = DBusProxy::new(&conn)
        .await
        .error("failed to cerate DBusProxy")?;
    proxy
        .add_match(
            "type='signal',\
                    path='/org/freedesktop/NetworkManager',\
                    interface='org.freedesktop.DBus.Properties',\
                    member='PropertiesChanged'",
        )
        .await
        .error("failed to add match")?;
    proxy
        .add_match(
            "type='signal',\
                    path_namespace='/org/freedesktop/NetworkManager/ActiveConnection',\
                    interface='org.freedesktop.DBus.Properties',\
                    member='PropertiesChanged'",
        )
        .await
        .error("failed to add match")?;
    Ok(conn.into())
}

/// DBus interface proxy for: `org.freedesktop.NetworkManager`
///
/// This code was generated by `zbus-xmlgen` `2.0.0` from DBus introspection data.
#[dbus_proxy(
    interface = "org.freedesktop.NetworkManager",
    default_path = "/org/freedesktop/NetworkManager",
    default_service = "org.freedesktop.NetworkManager"
)]
trait NetworkManager {
    /// ActiveConnections property
    #[dbus_proxy(property)]
    fn active_connections(&self) -> zbus::Result<Vec<OwnedObjectPath>>;

    /// PrimaryConnection property
    #[dbus_proxy(property)]
    fn primary_connection(&self) -> zbus::Result<OwnedObjectPath>;

    /// State property
    #[dbus_proxy(property)]
    fn state(&self) -> zbus::Result<u32>;
}

/// DBus interface proxy for: `org.freedesktop.NetworkManager.Connection.Active`
///
/// This code was generated by `zbus-xmlgen` `2.0.1` from DBus introspection data.
#[dbus_proxy(
    interface = "org.freedesktop.NetworkManager.Connection.Active",
    default_service = "org.freedesktop.NetworkManager"
)]
trait NetworkManagerConnection {
    /// Devices property
    #[dbus_proxy(property)]
    fn devices(&self) -> zbus::Result<Vec<OwnedObjectPath>>;

    /// Id property
    #[dbus_proxy(property)]
    fn id(&self) -> zbus::Result<StdString>;

    /// State property
    #[dbus_proxy(property)]
    fn state(&self) -> zbus::Result<u32>;

    /// Vpn property
    #[dbus_proxy(property)]
    fn vpn(&self) -> zbus::Result<bool>;
}

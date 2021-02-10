use std::collections::BTreeMap;
use std::fmt;
use std::net::Ipv4Addr;
use std::result;
use std::thread;
use std::time::Instant;

use crossbeam_channel::Sender;
use dbus::arg::{Array, Iter, Variant};
use dbus::{
    arg::messageitem::MessageItem,
    ffidisp::{BusType, Connection, ConnectionItem},
    Message, Path,
};
use regex::Regex;
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::Config;
use crate::errors::*;
use crate::scheduler::Task;
use crate::util::FormatTemplate;
use crate::widget::{I3BarWidget, Spacing, State};
use crate::widgets::button::ButtonWidget;

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
        match id {
            // https://developer.gnome.org/NetworkManager/stable/nm-dbus-types.html#NMState
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

enum ActiveConnectionState {
    Unknown,
    Activating,
    Activated,
    Deactivating,
    Deactivated,
}

impl From<u32> for ActiveConnectionState {
    fn from(id: u32) -> Self {
        match id {
            // https://developer.gnome.org/NetworkManager/stable/nm-dbus-types.html#NMActiveConnectionState
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
    TUN,
    Wireguard,
}

impl From<u32> for DeviceType {
    fn from(id: u32) -> Self {
        match id {
            // https://developer.gnome.org/NetworkManager/stable/nm-dbus-types.html#NMDeviceType
            1 => DeviceType::Ethernet,
            2 => DeviceType::Wifi,
            8 => DeviceType::Modem,
            13 => DeviceType::Bridge,
            16 => DeviceType::TUN,
            29 => DeviceType::Wireguard,
            _ => DeviceType::Unknown,
        }
    }
}

impl DeviceType {
    fn to_icon_name(&self) -> Option<String> {
        match self {
            DeviceType::Ethernet => Some("net_wired".to_string()),
            DeviceType::Wifi => Some("net_wireless".to_string()),
            DeviceType::Modem => Some("net_modem".to_string()),
            DeviceType::Bridge => Some("net_bridge".to_string()),
            DeviceType::TUN => Some("net_bridge".to_string()),
            DeviceType::Wireguard => Some("net_vpn".to_string()),
            _ => None,
        }
    }
}

#[derive(Debug)]
struct Ipv4Address {
    address: Ipv4Addr,
    prefix: u32,
    gateway: Ipv4Addr,
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

impl fmt::Display for Ipv4Address {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}/{}", self.address, self.prefix)
    }
}

struct ConnectionManager {}

impl ConnectionManager {
    pub fn new() -> Self {
        ConnectionManager {}
    }

    fn get(c: &Connection, path: Path, t: &str, property: &str) -> Result<Message> {
        let m = Message::new_method_call(
            "org.freedesktop.NetworkManager",
            path,
            "org.freedesktop.DBus.Properties",
            "Get",
        )
        .block_error("networkmanager", "Failed to create message")?
        .append2(
            MessageItem::Str(t.to_string()),
            MessageItem::Str(property.to_string()),
        );

        let r = c.send_with_reply_and_block(m, 1000);

        r.block_error("networkmanager", "Failed to retrieve property")
    }

    fn get_property(c: &Connection, property: &str) -> Result<Message> {
        Self::get(
            c,
            "/org/freedesktop/NetworkManager".into(),
            "org.freedesktop.NetworkManager",
            property,
        )
    }

    pub fn state(&self, c: &Connection) -> Result<NetworkState> {
        let m = Self::get_property(c, "State")
            .block_error("networkmanager", "Failed to retrieve state")?;

        let state: Variant<u32> = m
            .get1()
            .block_error("networkmanager", "Failed to read property")?;

        Ok(NetworkState::from(state.0))
    }

    pub fn primary_connection(&self, c: &Connection) -> Result<NmConnection> {
        let m = Self::get_property(c, "PrimaryConnection")
            .block_error("networkmanager", "Failed to retrieve primary connection")?;

        let primary_connection: Variant<Path> = m
            .get1()
            .block_error("networkmanager", "Failed to read primary connection")?;

        if let Ok(conn) = primary_connection.0.as_cstr().to_str() {
            if conn == "/" {
                return Err(BlockError(
                    "networkmanager".to_string(),
                    "No primary connection".to_string(),
                ));
            }
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

#[derive(Clone)]
struct NmConnection<'a> {
    path: Path<'a>,
}

impl<'a> NmConnection<'a> {
    fn state(&self, c: &Connection) -> Result<ActiveConnectionState> {
        let m = ConnectionManager::get(
            c,
            self.path.clone(),
            "org.freedesktop.NetworkManager.Connection.Active",
            "State",
        )
        .block_error("networkmanager", "Failed to retrieve connection state")?;

        let state: Variant<u32> = m
            .get1()
            .block_error("networkmanager", "Failed to read connection state")?;
        Ok(ActiveConnectionState::from(state.0))
    }

    fn id(&self, c: &Connection) -> Result<String> {
        let m = ConnectionManager::get(
            c,
            self.path.clone(),
            "org.freedesktop.NetworkManager.Connection.Active",
            "Id",
        )
        .block_error("networkmanager", "Failed to retrieve connection ID")?;

        let id: Variant<String> = m
            .get1()
            .block_error("networkmanager", "Failed to read Id")?;
        Ok(id.0)
    }

    fn devices(&self, c: &Connection) -> Result<Vec<NmDevice>> {
        let m = ConnectionManager::get(
            c,
            self.path.clone(),
            "org.freedesktop.NetworkManager.Connection.Active",
            "Devices",
        )
        .block_error("networkmanager", "Failed to retrieve connection device")?;

        let devices: Variant<Array<Path, Iter>> = m
            .get1()
            .block_error("networkmanager", "Failed to read devices")?;
        Ok(devices.0.map(|x| NmDevice { path: x }).collect())
    }
}

#[derive(Clone)]
struct NmDevice<'a> {
    path: Path<'a>,
}

impl<'a> NmDevice<'a> {
    fn device_type(&self, c: &Connection) -> Result<DeviceType> {
        let m = ConnectionManager::get(
            c,
            self.path.clone(),
            "org.freedesktop.NetworkManager.Device",
            "DeviceType",
        )
        .block_error("networkmanager", "Failed to retrieve device type")?;

        let device_type: Variant<u32> = m
            .get1()
            .block_error("networkmanager", "Failed to read device type")?;
        Ok(DeviceType::from(device_type.0))
    }

    fn interface_name(&self, c: &Connection) -> Result<String> {
        let m = ConnectionManager::get(
            c,
            self.path.clone(),
            "org.freedesktop.NetworkManager.Device",
            "Interface",
        )
        .block_error("networkmanager", "Failed to retrieve device interface name")?;

        let interface_name: Variant<String> = m
            .get1()
            .block_error("networkmanager", "Failed to read interface name")?;

        Ok(interface_name.0)
    }

    fn ip4config(&self, c: &Connection) -> Result<NmIp4Config> {
        let m = ConnectionManager::get(
            c,
            self.path.clone(),
            "org.freedesktop.NetworkManager.Device",
            "Ip4Config",
        )
        .block_error("networkmanager", "Failed to retrieve device ip4config")?;

        let ip4config: Variant<Path> = m
            .get1()
            .block_error("networkmanager", "Failed to read ip4config")?;
        Ok(NmIp4Config { path: ip4config.0 })
    }

    fn active_access_point(&self, c: &Connection) -> Result<NmAccessPoint> {
        let m = ConnectionManager::get(
            c,
            self.path.clone(),
            "org.freedesktop.NetworkManager.Device.Wireless",
            "ActiveAccessPoint",
        )
        .block_error(
            "networkmanager",
            "Failed to retrieve device active access point",
        )?;

        let active_ap: Variant<Path> = m
            .get1()
            .block_error("networkmanager", "Failed to read active access point")?;
        Ok(NmAccessPoint { path: active_ap.0 })
    }
}

#[derive(Clone)]
struct NmAccessPoint<'a> {
    path: Path<'a>,
}

impl<'a> NmAccessPoint<'a> {
    fn ssid(&self, c: &Connection) -> Result<String> {
        let m = ConnectionManager::get(
            c,
            self.path.clone(),
            "org.freedesktop.NetworkManager.AccessPoint",
            "Ssid",
        )
        .block_error("networkmanager", "Failed to retrieve SSID")?;

        let ssid: Variant<Array<u8, Iter>> = m
            .get1()
            .block_error("networkmanager", "Failed to read ssid")?;
        Ok(std::str::from_utf8(&ssid.0.collect::<Vec<u8>>())
            .block_error("networkmanager", "Failed to parse ssid")?
            .to_string())
    }

    fn strength(&self, c: &Connection) -> Result<u8> {
        let m = ConnectionManager::get(
            c,
            self.path.clone(),
            "org.freedesktop.NetworkManager.AccessPoint",
            "Strength",
        )
        .block_error("networkmanager", "Failed to retrieve strength")?;

        let strength: Variant<u8> = m
            .get1()
            .block_error("networkmanager", "Failed to read strength")?;
        Ok(strength.0)
    }

    fn frequency(&self, c: &Connection) -> Result<u32> {
        let m = ConnectionManager::get(
            c,
            self.path.clone(),
            "org.freedesktop.NetworkManager.AccessPoint",
            "Frequency",
        )
        .block_error("networkmanager", "Failed to retrieve frequency")?;

        let frequency: Variant<u32> = m
            .get1()
            .block_error("networkmanager", "Failed to read frequency")?;
        Ok(frequency.0)
    }
}

#[derive(Clone)]
struct NmIp4Config<'a> {
    path: Path<'a>,
}

impl<'a> NmIp4Config<'a> {
    fn addresses(&self, c: &Connection) -> Result<Vec<Ipv4Address>> {
        let m = ConnectionManager::get(
            c,
            self.path.clone(),
            "org.freedesktop.NetworkManager.IP4Config",
            "Addresses",
        )
        .block_error("networkmanager", "Failed to retrieve addresses")?;

        let addresses: Variant<Array<Array<u32, Iter>, Iter>> = m
            .get1()
            .block_error("networkmanager", "Failed to read addresses")?;
        Ok(addresses.0.map(Ipv4Address::from).collect())
    }
}

pub struct NetworkManager {
    id: usize,
    indicator: ButtonWidget,
    output: Vec<ButtonWidget>,
    dbus_conn: Connection,
    manager: ConnectionManager,
    config: Config,
    primary_only: bool,
    max_ssid_width: usize,
    ap_format: FormatTemplate,
    device_format: FormatTemplate,
    connection_format: FormatTemplate,
    interface_name_exclude_regexps: Vec<Regex>,
    interface_name_include_regexps: Vec<Regex>,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct NetworkManagerConfig {
    /// Whether to only show the primary connection, or all active connections.
    #[serde(default = "NetworkManagerConfig::default_primary_only")]
    pub primary_only: bool,

    /// Max SSID width, in characters.
    #[serde(default = "NetworkManagerConfig::default_max_ssid_width")]
    pub max_ssid_width: usize,

    /// AP formatter
    #[serde(default = "NetworkManagerConfig::default_ap_format")]
    pub ap_format: String,

    /// Device formatter.
    #[serde(default = "NetworkManagerConfig::default_device_format")]
    pub device_format: String,

    /// Connection formatter.
    #[serde(default = "NetworkManagerConfig::default_connection_format")]
    pub connection_format: String,

    /// Interface name regex patterns to include.
    #[serde(default = "NetworkManagerConfig::default_interface_name_include_patterns")]
    pub interface_name_exclude: Vec<String>,

    /// Interface name regex patterns to ignore.
    #[serde(default = "NetworkManagerConfig::default_interface_name_exclude_patterns")]
    pub interface_name_include: Vec<String>,

    #[serde(default = "NetworkManagerConfig::default_color_overrides")]
    pub color_overrides: Option<BTreeMap<String, String>>,
}

impl NetworkManagerConfig {
    fn default_primary_only() -> bool {
        false
    }

    fn default_max_ssid_width() -> usize {
        21
    }

    fn default_ap_format() -> String {
        "{ssid}".to_string()
    }

    fn default_device_format() -> String {
        "{icon}{ap} {ips}".to_string()
    }

    fn default_connection_format() -> String {
        "{devices}".to_string()
    }

    fn default_interface_name_include_patterns() -> Vec<String> {
        vec![]
    }

    fn default_interface_name_exclude_patterns() -> Vec<String> {
        vec![]
    }

    fn default_color_overrides() -> Option<BTreeMap<String, String>> {
        None
    }
}

impl ConfigBlock for NetworkManager {
    type Config = NetworkManagerConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        config: Config,
        send: Sender<Task>,
    ) -> Result<Self> {
        let dbus_conn = Connection::get_private(BusType::System)
            .block_error("networkmanager", "failed to establish D-Bus connection")?;
        let manager = ConnectionManager::new();

        thread::Builder::new()
            .name("networkmanager".into())
            .spawn(move || {
                let c = Connection::get_private(BusType::System).unwrap();
                let rule = "type='signal',\
                        path='/org/freedesktop/NetworkManager',\
                        interface='org.freedesktop.NetworkManager',\
                        member='PropertiesChanged'";

                c.add_match(&rule).unwrap();

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
            patterns.iter().map(|p| Regex::new(&p)).collect()
        }

        Ok(NetworkManager {
            id,
            config: config.clone(),
            indicator: ButtonWidget::new(config, id),
            output: Vec::new(),
            dbus_conn,
            manager,
            primary_only: block_config.primary_only,
            max_ssid_width: block_config.max_ssid_width,
            ap_format: FormatTemplate::from_string(&block_config.ap_format)?,
            device_format: FormatTemplate::from_string(&block_config.device_format)?,
            connection_format: FormatTemplate::from_string(&block_config.connection_format)?,
            interface_name_exclude_regexps: compile_regexps(block_config.interface_name_exclude)
                .block_error("networkmanager", "failed to parse exclude patterns")?,
            interface_name_include_regexps: compile_regexps(block_config.interface_name_include)
                .block_error("networkmanager", "failed to parse include patterns")?,
        })
    }
}

impl Block for NetworkManager {
    fn id(&self) -> usize {
        self.id
    }

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
            Ok(NetworkState::Disconnected) => "×",
            Ok(NetworkState::Asleep) => "×",
            Ok(NetworkState::Unknown) => "E",
            _ => "",
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
                            .chain(active.into_iter().filter(|ref x| x.path != conn.path))
                            .collect(),
                        Err(_) => active,
                    }
                };

                connections
                    .into_iter()
                    .filter_map(|conn| {
                        // inline spacing for no leading space, because the icon is set in the string
                        let mut widget = ButtonWidget::new(self.config.clone(), self.id)
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

                                let (icon, type_name) = if let Ok(dev_type) =
                                    device.device_type(&self.dbus_conn)
                                {
                                    match dev_type.to_icon_name() {
                                        Some(icon_name) => {
                                            let i = self
                                                .config
                                                .icons
                                                .get(&icon_name)
                                                .cloned()
                                                .unwrap_or_else(|| "".to_string());
                                            (i.to_string(), format!("{:?}", dev_type).to_string())
                                        }
                                        None => (
                                            self.config
                                                .icons
                                                .get("unknown")
                                                .cloned()
                                                .unwrap_or_else(|| "".to_string()),
                                            format!("{:?}", dev_type).to_string(),
                                        ),
                                    }
                                } else {
                                    // TODO: Communicate the error to the user?
                                    ("".to_string(), "".to_string())
                                };

                                let ap = if let Ok(ap) = device.active_access_point(&self.dbus_conn)
                                {
                                    let ssid = match ap.ssid(&self.dbus_conn) {
                                        Ok(ssid) => {
                                            let mut truncated = ssid.to_string();
                                            truncated.truncate(self.max_ssid_width);
                                            truncated
                                        }
                                        Err(_) => "".to_string(),
                                    };
                                    let strength = match ap.strength(&self.dbus_conn) {
                                        Ok(v) => format!("{}", v).to_string(),
                                        Err(_) => "0".to_string(),
                                    };
                                    let freq = match ap.frequency(&self.dbus_conn) {
                                        Ok(v) => format!("{}", v).to_string(),
                                        Err(_) => "0".to_string(),
                                    };

                                    let values = map!("{ssid}" => ssid,
                                                      "{strength}" => strength,
                                                      "{freq}" => freq);
                                    if let Ok(s) = self.ap_format.render_static_str(&values) {
                                        s
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

                                let values = map!("{icon}" => icon,
                                                  "{typename}" => type_name,
                                                  "{ap}" => ap,
                                                  "{name}" => name.to_string(), 
                                                  "{ips}" => ips);

                                if let Ok(s) = self.device_format.render_static_str(&values) {
                                    devicevec.push(s);
                                } else {
                                    devicevec.push("[invalid device format string]".to_string())
                                }
                            }
                        };

                        let id = match conn.id(&self.dbus_conn) {
                            Ok(id) => id,
                            Err(v) => format!("{:?}", v),
                        };

                        let values = map!("{devices}" => devicevec.join(" "),
                                          "{id}" => id);

                        if let Ok(s) = self.connection_format.render_static_str(&values) {
                            widget.set_text(s);
                        } else {
                            widget.set_text("[invalid connection format string]");
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

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        if self.output.is_empty() {
            vec![&self.indicator]
        } else {
            self.output.iter().map(|x| x as &dyn I3BarWidget).collect()
        }
    }
}

use std::fmt;
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use dbus::arg::Variant;
use dbus::{
    ffidisp::{BusType, Connection},
    Message,
};
use serde_derive::Deserialize;
use uuid::Uuid;

use crate::blocks::{Block, ConfigBlock};
use crate::config::Config;
use crate::errors::*;
use crate::scheduler::Task;
use crate::widget::{I3BarWidget, State};
use crate::widgets::text::TextWidget;

enum NetworkState {
    Unknown = 0,
    Asleep = 10,
    Disconnected = 20,
    Disconnecting = 30,
    Connecting = 40,
    ConnectedLocal = 50,
    ConnectedSite = 60,
    ConnectedGlobal = 70,
}

impl From<u32> for NetworkState {
    fn from(id: u32) -> Self {
        match id {
            // https://developer.gnome.org/NetworkManager/unstable/nm-dbus-types.html#NMState
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

impl fmt::Display for NetworkState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            NetworkState::Unknown => write!(f, "DOWN"),
            NetworkState::Asleep => write!(f, "DOWN"),
            NetworkState::Disconnected => write!(f, "DOWN"),
            NetworkState::Disconnecting => write!(f, "DOWN"),
            NetworkState::Connecting => write!(f, "DOWN"),
            NetworkState::ConnectedLocal => write!(f, "UP"),
            NetworkState::ConnectedSite => write!(f, "UP"),
            NetworkState::ConnectedGlobal => write!(f, "UP"),
        }
    }
}

enum ConnectionType {
    Ethernet,
    Wireless,
    Other,
}

impl From<String> for ConnectionType {
    fn from(name: String) -> Self {
        match name.as_ref() {
            // https://developer.gnome.org/NetworkManager/unstable/settings-connection.html
            "802-3-ethernet" => ConnectionType::Ethernet,
            "802-11-wireless" => ConnectionType::Wireless,
            _ => ConnectionType::Other,
        }
    }
}

impl fmt::Display for ConnectionType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ConnectionType::Ethernet => write!(f, "net_wired"),
            ConnectionType::Wireless => write!(f, "net_wireless"),
            ConnectionType::Other => write!(f, "net_other"),
        }
    }
}

struct ConnectionManager {}

impl ConnectionManager {
    pub fn new() -> Self {
        ConnectionManager {}
    }

    fn get_property(c: &Connection, property: &str) -> Result<Message> {
        let m = Message::new_method_call(
            "org.freedesktop.NetworkManager",
            "/org/freedesktop/NetworkManager",
            "org.freedesktop.DBus.Properties",
            "Get",
        )
        .block_error("networkmanager", "Failed to create message")?
        .append2("org.freedesktop.NetworkManager", property);

        let r = c.send_with_reply_and_block(m, 1000);

        r.block_error("networkmanager", "Failed to retrieve property")
    }

    pub fn state(&self, c: &Connection) -> Result<NetworkState> {
        let m = Self::get_property(c, "State")?;

        let state: Variant<u32> = m
            .get1()
            .block_error("networkmanager", "Failed to read property")?;

        Ok(NetworkState::from(state.0))
    }

    pub fn connection_type(&self, c: &Connection) -> Result<ConnectionType> {
        let m = Self::get_property(c, "PrimaryConnectionType")?;

        let connection_type: Variant<String> = m
            .get1()
            .block_error("networkmanager", "Failed to read property")?;

        Ok(ConnectionType::from(connection_type.0))
    }
}

pub struct NetworkManager {
    id: String,
    output: TextWidget,
    dbus_conn: Connection,
    manager: ConnectionManager,
    show_type: bool,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct NetworkManagerConfig {
    /// Whether to show the connection type or not.
    #[serde(default = "NetworkManagerConfig::default_show_type")]
    pub show_type: bool,
}

impl NetworkManagerConfig {
    fn default_show_type() -> bool {
        true
    }
}

impl ConfigBlock for NetworkManager {
    type Config = NetworkManagerConfig;

    fn new(block_config: Self::Config, config: Config, send: Sender<Task>) -> Result<Self> {
        let id: String = Uuid::new_v4().to_simple().to_string();
        let id_copy = id.clone();
        let dbus_conn = Connection::get_private(BusType::System)
            .block_error("networkmanager", "failed to establish D-Bus connection")?;
        let manager = ConnectionManager::new();

        thread::spawn(move || {
            let c = Connection::get_private(BusType::System).unwrap();
            let rule = "type='signal',\
                        path='/org/freedesktop/NetworkManager',\
                        interface='org.freedesktop.NetworkManager',\
                        member='StateChanged'";

            c.add_match(&rule).unwrap();

            loop {
                let timeout = 100_000;

                for _event in c.iter(timeout) {
                    send.send(Task {
                        id: id.clone(),
                        update_time: Instant::now(),
                    })
                    .unwrap();
                }
            }
        });

        Ok(NetworkManager {
            id: id_copy,
            output: TextWidget::new(config),
            dbus_conn,
            manager,
            show_type: block_config.show_type,
        })
    }
}

impl Block for NetworkManager {
    fn id(&self) -> &str {
        &self.id
    }

    fn update(&mut self) -> Result<Option<Duration>> {
        let state = self.manager.state(&self.dbus_conn)?;
        let connection_type = self.manager.connection_type(&self.dbus_conn)?;

        self.output.set_icon(&connection_type.to_string());
        self.output.set_state(match state {
            NetworkState::ConnectedGlobal => State::Good,
            NetworkState::ConnectedSite => State::Info,
            NetworkState::ConnectedLocal => State::Idle,
            NetworkState::Connecting => State::Warning,
            NetworkState::Disconnecting => State::Warning,
            NetworkState::Asleep => State::Warning,
            NetworkState::Disconnected => State::Critical,
            NetworkState::Unknown => State::Critical,
        });

        if self.show_type {
            self.output.set_text(state.to_string());
        }

        Ok(None)
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.output]
    }
}

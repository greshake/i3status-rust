//! Monitor Bluetooth device
//!
//! This block displays the connectivity of a given Bluetooth device and the battery level if this
//! is supported. Relies on the Bluez D-Bus API.
//!
//! When the device can be identified as an audio headset, a keyboard, joystick, or mouse, use the
//! relevant icon. Otherwise, fall back on the generic Bluetooth symbol.
//!
//! Right-clicking the block will attempt to connect (or disconnect) the device.
//!
//! # Configuration
//!
//! Key | Values | Required | Default
//! ----|--------|----------|--------
//! `mac` | MAC address of the Bluetooth device | Yes | -
//! `adapter_mac` | MAC Address of the Bluetooth adapter (in case your device was connected to multiple currently available adapters) | No | None
//! `format` | A string to customise the output of this block. See below for available placeholders. | No | <code>"$name{ $percentage&vert;}&vert;Unavailable"</code>
//! `hide_disconnected` | Whether to hide the block when disconnected | No | `false`
//!
//! Placeholder  | Value                                                                 | Type   | Unit
//! -------------|-----------------------------------------------------------------------|--------|------
//! `name`       | Device's name                                                         | Text   | -
//! `percentage` | Device's battery level (may be absent if the device is not supported) | Number | %
//! `available`  | Present if the device is available                                    | Flag   | -
//! `connected`  | Present if the device is connected                                    | Flag   | -
//!
//! # Examples
//!
//! This example just shows the icon when device is connected.
//!
//! ```toml
//! [[block]]
//! block = "bluetooth"
//! mac = "00:18:09:92:1B:BA"
//! hide_disconnected = true
//! format = ""
//! ```
//!
//! # Icons Used
//! - `headphones` for bluetooth devices identifying as "audio-card" or "audio-headset"
//! - `joystick` for bluetooth devices identifying as "input-gaming"
//! - `keyboard` for bluetooth devices identifying as "input-keyboard"
//! - `mouse` for bluetooth devices identifying as "input-mouse"
//! - `bluetooth` for all other devices

use super::prelude::*;
use zbus::fdo::{
    InterfacesAddedStream, InterfacesRemovedStream, ObjectManagerProxy, PropertiesProxy,
};

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct BluetoothConfig {
    mac: String,
    #[serde(default)]
    adapter_mac: Option<String>,
    #[serde(default)]
    format: FormatConfig,
    #[serde(default)]
    hide_disconnected: bool,
}

pub async fn run(config: toml::Value, mut api: CommonApi) -> Result<()> {
    let mut events = api.get_events().await?;
    let config = BluetoothConfig::deserialize(config).config_error()?;
    api.set_format(
        config
            .format
            .with_default("$name{ $percentage|}|Unavailable")?,
    );

    let dbus_conn = api.get_system_dbus_connection().await?;
    let mut monitor = DeviceMonitor::new(&dbus_conn, config.mac, config.adapter_mac).await?;

    loop {
        match monitor.device() {
            // Available
            Some(device) => {
                let connected = device.connected().await?;
                if connected || !config.hide_disconnected {
                    let mut values = map! {
                        "name" => Value::text(device.name().await?),
                        "available" => Value::Flag,
                    };
                    device
                        .percentage()
                        .await
                        .map(|p| values.insert("percentage".into(), Value::percents(p)));
                    if connected {
                        api.set_state(State::Good);
                        values.insert("connected".into(), Value::Flag);
                    } else {
                        api.set_state(State::Idle);
                    }
                    api.set_icon(device.icon().await?)?;
                    api.set_values(values);
                    api.show();
                } else {
                    api.hide();
                }
            }
            // Unavailable
            None => {
                if !config.hide_disconnected {
                    api.set_icon("bluetooth")?;
                    api.set_state(State::Idle);
                    api.set_values(map!());
                    api.show();
                } else {
                    api.hide();
                }
            }
        }

        api.flush().await?;

        loop {
            tokio::select! {
                Some(BlockEvent::Click(click)) = events.recv() => {
                    if click.button == MouseButton::Right {
                        if let Some(dev) = monitor.device() {
                            if let Ok(connected) = dev.connected().await {
                                if connected {
                                    let _ = dev.device.disconnect().await;
                                } else {
                                    let _ = dev.device.connect().await;
                                }
                            }
                        }
                    }
                }
                res = monitor.wait_for_change() => {
                    res?;
                    break;
                },
            }
        }
    }
}

struct DeviceMonitor {
    mac: String,
    adapter_mac: Option<String>,
    manager_proxy: ObjectManagerProxy<'static>,
    device: Option<Device>,
    interface_added: InterfacesAddedStream<'static>,
    interface_removed: InterfacesRemovedStream<'static>,
}

#[derive(Clone)]
struct Device {
    available: bool,
    props: PropertiesProxy<'static>,
    device: Device1Proxy<'static>,
    battery: Option<Battery1Proxy<'static>>,
}

impl DeviceMonitor {
    async fn new(
        dbus_conn: &zbus::Connection,
        mac: String,
        adapter_mac: Option<String>,
    ) -> Result<Self> {
        let manager_proxy = ObjectManagerProxy::builder(dbus_conn)
            .destination("org.bluez")
            .and_then(|x| x.path("/"))
            .unwrap()
            .build()
            .await
            .error("Failed to create ObjectManagerProxy")?;
        let interface_added = manager_proxy
            .receive_interfaces_added()
            .await
            .error("Failed to monitor interfaces")?;
        let interface_removed = manager_proxy
            .receive_interfaces_removed()
            .await
            .error("Failed to monitor interfaces")?;
        let device = Device::try_find(&manager_proxy, &mac, adapter_mac.as_deref()).await?;
        Ok(Self {
            mac,
            adapter_mac,
            manager_proxy,
            device,
            interface_added,
            interface_removed,
        })
    }

    fn device(&self) -> Option<&Device> {
        match &self.device {
            Some(device) if device.available => Some(device),
            _ => None,
        }
    }

    async fn wait_for_change(&mut self) -> Result<()> {
        match &mut self.device {
            None => loop {
                tokio::select! {
                    _ = self.interface_added.next() => {
                        if let Some(device) = Device::try_find(
                            &self.manager_proxy,
                            &self.mac,
                            self.adapter_mac.as_deref()
                        ).await?
                        {
                            self.device = Some(device);
                            return Ok(());
                        }
                    }
                    _ = self.interface_removed.next() => (),
                }
            },
            Some(device) if !device.available => loop {
                tokio::select! {
                    Some(event) = self.interface_added.next() => {
                        let args = event.args().error("Failed to get the args")?;
                        if args.object_path() == device.device.path() {
                            device.available = true;
                            return Ok(());
                        }
                    }
                    _ = self.interface_removed.next() => (),
                }
            },
            Some(device) => {
                let mut updates = device
                    .props
                    .receive_properties_changed()
                    .await
                    .error("Failed to receive updates")?;
                loop {
                    tokio::select! {
                        _ = updates.next() => {
                            // avoid too frequent updates
                            let _ = tokio::time::timeout(Duration::from_millis(100), async {
                                loop { let _ = updates.next().await; }
                            }).await;
                            return Ok(());
                        }
                        _ = self.interface_added.next() => (),
                        Some(event) = self.interface_removed.next() => {
                            let args = event.args().error("Failed to get the args")?;
                            if args.object_path() == device.device.path() {
                                device.available = false;
                                return Ok(());
                            }
                        },
                    }
                }
            }
        }
    }
}

impl Device {
    async fn try_find(
        manager_proxy: &ObjectManagerProxy<'_>,
        mac: &str,
        adapter_mac: Option<&str>,
    ) -> Result<Option<Self>> {
        let root_oject: String = match adapter_mac {
            Some(adapter_mac) => {
                let adapters = manager_proxy
                    .get_managed_objects()
                    .await
                    .error("Failed to get a list of adapters")?;
                let mut adapter_path = None;
                for (path, interfaces) in adapters {
                    let adapter_interface = match interfaces.get("org.bluez.Adapter1") {
                        Some(i) => i,
                        None => continue, // Not an adapter
                    };
                    let addr: &str = adapter_interface
                        .get("Address")
                        .and_then(|a| a.downcast_ref())
                        .unwrap();
                    if addr == adapter_mac {
                        adapter_path = Some(path);
                        break;
                    }
                }
                match adapter_path {
                    Some(path) => path.as_str().into(),
                    None => return Ok(None),
                }
            }
            None => String::new(),
        };

        // Iterate over all devices
        let devices = manager_proxy
            .get_managed_objects()
            .await
            .error("Failed to get the list of devices")?;
        for (path, interfaces) in devices {
            if !path.starts_with(&format!("{}/", root_oject)) {
                continue;
            }

            let device_interface = match interfaces.get("org.bluez.Device1") {
                Some(i) => i,
                None => continue, // Not a device
            };

            let addr: &str = device_interface
                .get("Address")
                .and_then(|a| a.downcast_ref())
                .unwrap();
            if addr != mac {
                continue;
            }

            return Ok(Some(Self {
                available: true,
                props: PropertiesProxy::builder(manager_proxy.connection())
                    .destination("org.bluez")
                    .and_then(|x| x.path(path.clone()))
                    .unwrap()
                    .build()
                    .await
                    .error("Failed to create PropertiesProxy")?,
                device: Device1Proxy::builder(manager_proxy.connection())
                    .path(path.clone())
                    .unwrap()
                    .build()
                    .await
                    .error("Failed to create Device1Proxy")?,
                battery: if interfaces.get("ogr.bluez.Battery1").is_some() {
                    Some(
                        Battery1Proxy::builder(manager_proxy.connection())
                            .path(path)
                            .unwrap()
                            .build()
                            .await
                            .error("Failed to create Battery1Proxy")?,
                    )
                } else {
                    None
                },
            }));
        }
        Ok(None)
    }

    async fn icon(&self) -> Result<&'static str> {
        self.device
            .icon()
            .await
            .map(|icon| match icon.as_str() {
                "audio-card" | "audio-headset" => "headphones",
                "input-gaming" => "joystick",
                "input-keyboard" => "keyboard",
                "input-mouse" => "mouse",
                _ => "bluetooth",
            })
            .error("Failed to get icon")
    }

    async fn name(&self) -> Result<String> {
        self.device
            .name()
            .await
            .map(Into::into)
            .error("Failed to get name")
    }

    async fn connected(&self) -> Result<bool> {
        self.device
            .connected()
            .await
            .error("Failed to get connected state")
    }

    async fn percentage(&self) -> Option<u8> {
        if let Some(bp) = &self.battery {
            bp.percentage().await.ok()
        } else {
            None
        }
    }
}

#[zbus::dbus_proxy(interface = "org.bluez.Device1", default_service = "org.bluez")]
trait Device1 {
    fn connect(&self) -> zbus::Result<()>;
    fn disconnect(&self) -> zbus::Result<()>;

    #[dbus_proxy(property)]
    fn connected(&self) -> zbus::Result<bool>;

    #[dbus_proxy(property)]
    fn name(&self) -> zbus::Result<StdString>;

    #[dbus_proxy(property)]
    fn icon(&self) -> zbus::Result<StdString>;
}

#[zbus::dbus_proxy(interface = "org.bluez.Battery1", default_service = "org.bluez")]
trait Battery1 {
    #[dbus_proxy(property)]
    fn percentage(&self) -> zbus::Result<u8>;
}

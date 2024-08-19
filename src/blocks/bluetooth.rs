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
//! Note: battery level information is not reported for some devices. [Enabling experimental
//! features of `bluez`](https://wiki.archlinux.org/title/bluetooth#Enabling_experimental_features)
//! may fix it.
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `mac` | MAC address of the Bluetooth device | **Required**
//! `adapter_mac` | MAC Address of the Bluetooth adapter (in case your device was connected to multiple currently available adapters) | `None`
//! `format` | A string to customise the output of this block. See below for available placeholders. | <code>\" $icon $name{ $percentage\|} \"</code>
//! `disconnected_format` | A string to customise the output of this block. See below for available placeholders. | <code>\" $icon{ $name\|} \"</code>
//! `battery_state` | A mapping from battery percentage to block's [state](State) (color). See example below. | 0..15 -> critical, 16..30 -> warning, 31..60 -> info, 61..100 -> good
//!
//! Placeholder    | Value                                                                 | Type   | Unit
//! ---------------|-----------------------------------------------------------------------|--------|------
//! `icon`         | Icon based on what type of device is connected                        | Icon   | -
//! `name`         | Device's name                                                         | Text   | -
//! `percentage`   | Device's battery level (may be absent if the device is not supported) | Number | %
//! `battery_icon` | Battery icon (may be absent if the device is not supported)           | Icon   | -
//! `available`    | Present if the device is available                                    | Flag   | -
//!
//! Action   | Default button
//! ---------|---------------
//! `toggle` | Right
//!
//! # Examples
//!
//! This example just shows the icon when device is connected.
//!
//! ```toml
//! [[block]]
//! block = "bluetooth"
//! mac = "00:18:09:92:1B:BA"
//! disconnected_format = ""
//! format = " $icon "
//! [block.battery_state]
//! "0..20" = "critical"
//! "21..70" = "warning"
//! "71..100" = "good"
//! ```
//!
//! # Icons Used
//! - `headphones` for bluetooth devices identifying as "audio-card", "audio-headset" or "audio-headphones"
//! - `joystick` for bluetooth devices identifying as "input-gaming"
//! - `keyboard` for bluetooth devices identifying as "input-keyboard"
//! - `mouse` for bluetooth devices identifying as "input-mouse"
//! - `bluetooth` for all other devices

use zbus::fdo::{DBusProxy, ObjectManagerProxy, PropertiesProxy};

use super::prelude::*;
use crate::wrappers::RangeMap;

make_log_macro!(debug, "bluetooth");

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub mac: String,
    #[serde(default)]
    pub adapter_mac: Option<String>,
    #[serde(default)]
    pub format: FormatConfig,
    #[serde(default)]
    pub disconnected_format: FormatConfig,
    #[serde(default)]
    pub battery_state: Option<RangeMap<u8, State>>,
}

pub async fn run(config: &Config, api: &CommonApi) -> Result<()> {
    let mut actions = api.get_actions()?;
    api.set_default_actions(&[(MouseButton::Right, None, "toggle")])?;

    let format = config.format.with_default(" $icon $name{ $percentage|} ")?;
    let disconnected_format = config
        .disconnected_format
        .with_default(" $icon{ $name|} ")?;

    let mut monitor = DeviceMonitor::new(config.mac.clone(), config.adapter_mac.clone()).await?;

    let battery_states = config.battery_state.clone().unwrap_or_else(|| {
        vec![
            (0..=15, State::Critical),
            (16..=30, State::Warning),
            (31..=60, State::Info),
            (61..=100, State::Good),
        ]
        .into()
    });

    loop {
        match monitor.get_device_info().await {
            // Available
            Some(device) => {
                debug!("Device available, info: {device:?}");

                let mut widget = Widget::new();

                let values = map! {
                    "icon" => Value::icon(device.icon),
                    "name" => Value::text(device.name),
                    "available" => Value::flag(),
                    [if let Some(p) = device.battery_percentage] "percentage" => Value::percents(p),
                    [if let Some(p) = device.battery_percentage]
                        "battery_icon" => Value::icon_progression("bat", p as f64 / 100.0),
                };

                if device.connected {
                    widget.set_format(format.clone());
                    widget.state = battery_states
                        .get(&device.battery_percentage.unwrap_or(100))
                        .copied()
                        .unwrap_or(State::Good);
                } else {
                    widget.set_format(disconnected_format.clone());
                    widget.state = State::Idle;
                }

                widget.set_values(values);
                api.set_widget(widget)?;
            }
            // Unavailable
            None => {
                debug!("Showing device as unavailable");
                let mut widget = Widget::new().with_format(disconnected_format.clone());
                widget.set_values(map!("icon" => Value::icon("bluetooth")));
                api.set_widget(widget)?;
            }
        }

        loop {
            select! {
                res = monitor.wait_for_change() => {
                    res?;
                    break;
                },
                Some(action) = actions.recv() => match action.as_ref() {
                    "toggle" => {
                        if let Some(dev) = &monitor.device {
                            if let Ok(connected) = dev.device.connected().await {
                                if connected {
                                    let _ = dev.device.disconnect().await;
                                } else {
                                    let _ = dev.device.connect().await;
                                }
                                break;
                            }
                        }
                    }
                    _ => (),
                }
            }
        }
    }
}

struct DeviceMonitor {
    mac: String,
    adapter_mac: Option<String>,
    manager_proxy: ObjectManagerProxy<'static>,
    device: Option<Device>,
}

struct Device {
    props: PropertiesProxy<'static>,
    device: Device1Proxy<'static>,
    battery: Battery1Proxy<'static>,
}

#[derive(Debug)]
struct DeviceInfo {
    connected: bool,
    icon: &'static str,
    name: String,
    battery_percentage: Option<u8>,
}

impl DeviceMonitor {
    async fn new(mac: String, adapter_mac: Option<String>) -> Result<Self> {
        let dbus_conn = new_system_dbus_connection().await?;
        let manager_proxy = ObjectManagerProxy::builder(&dbus_conn)
            .destination("org.bluez")
            .and_then(|x| x.path("/"))
            .unwrap()
            .build()
            .await
            .error("Failed to create ObjectManagerProxy")?;
        let device = Device::try_find(&manager_proxy, &mac, adapter_mac.as_deref()).await?;
        Ok(Self {
            mac,
            adapter_mac,
            manager_proxy,
            device,
        })
    }

    async fn wait_for_change(&mut self) -> Result<()> {
        match &mut self.device {
            None => {
                let mut interface_added = self
                    .manager_proxy
                    .receive_interfaces_added()
                    .await
                    .error("Failed to monitor interfaces")?;
                loop {
                    interface_added
                        .next()
                        .await
                        .error("Stream ended unexpectedly")?;
                    if let Some(device) = Device::try_find(
                        &self.manager_proxy,
                        &self.mac,
                        self.adapter_mac.as_deref(),
                    )
                    .await?
                    {
                        self.device = Some(device);
                        debug!("Device has been added");
                        return Ok(());
                    }
                }
            }
            Some(device) => {
                let mut updates = device
                    .props
                    .receive_properties_changed()
                    .await
                    .error("Failed to receive updates")?;

                let mut interface_added = self
                    .manager_proxy
                    .receive_interfaces_added()
                    .await
                    .error("Failed to monitor interfaces")?;

                let mut interface_removed = self
                    .manager_proxy
                    .receive_interfaces_removed()
                    .await
                    .error("Failed to monitor interfaces")?;

                let mut bluez_owner_changed =
                    DBusProxy::new(self.manager_proxy.inner().connection())
                        .await
                        .error("Failed to create DBusProxy")?
                        .receive_name_owner_changed_with_args(&[(0, "org.bluez")])
                        .await
                        .unwrap();

                loop {
                    select! {
                        _ = updates.next() => {
                            // avoid too frequent updates
                            let _ = tokio::time::timeout(Duration::from_millis(100), async {
                                loop { let _ = updates.next().await; }
                            }).await;
                            debug!("Got update for device");
                            return Ok(());
                        }
                        Some(event) = interface_added.next() => {
                            let args = event.args().error("Failed to get the args")?;
                            if args.object_path() == device.device.inner().path() {
                                debug!("Interfaces added: {:?}", args.interfaces_and_properties().keys());
                                return Ok(());
                            }
                        }
                        Some(event) = interface_removed.next() => {
                            let args = event.args().error("Failed to get the args")?;
                            if args.object_path() == device.device.inner().path() {
                                self.device = None;
                                debug!("Device is no longer available");
                                return Ok(());
                            }
                        }
                        Some(event) = bluez_owner_changed.next() => {
                            let args = event.args().error("Failed to get the args")?;
                            if args.new_owner.is_none() {
                                self.device = None;
                                debug!("org.bluez disappeared");
                                return Ok(());
                            }
                        }
                    }
                }
            }
        }
    }

    async fn get_device_info(&mut self) -> Option<DeviceInfo> {
        let device = self.device.as_ref()?;

        let Ok((connected, name)) =
            tokio::try_join!(device.device.connected(), device.device.name(),)
        else {
            debug!("failed to fetch device info, assuming device or bluez disappeared");
            self.device = None;
            return None;
        };

        //icon can be null, so ignore errors when fetching it
        let icon: &str = match device.device.icon().await.ok().as_deref() {
            Some("audio-card" | "audio-headset" | "audio-headphones") => "headphones",
            Some("input-gaming") => "joystick",
            Some("input-keyboard") => "keyboard",
            Some("input-mouse") => "mouse",
            _ => "bluetooth",
        };

        Some(DeviceInfo {
            connected,
            icon,
            name,
            battery_percentage: device.battery.percentage().await.ok(),
        })
    }
}

impl Device {
    async fn try_find(
        manager_proxy: &ObjectManagerProxy<'_>,
        mac: &str,
        adapter_mac: Option<&str>,
    ) -> Result<Option<Self>> {
        let Ok(devices) = manager_proxy.get_managed_objects().await else {
            debug!("could not get the list of managed objects");
            return Ok(None);
        };

        debug!("all managed devices: {:?}", devices);

        let root_object: Option<String> = match adapter_mac {
            Some(adapter_mac) => {
                let mut adapter_path = None;
                for (path, interfaces) in &devices {
                    let adapter_interface = match interfaces.get("org.bluez.Adapter1") {
                        Some(i) => i,
                        None => continue, // Not an adapter
                    };
                    let addr: &str = adapter_interface
                        .get("Address")
                        .and_then(|a| a.downcast_ref().ok())
                        .unwrap();
                    if addr == adapter_mac {
                        adapter_path = Some(path);
                        break;
                    }
                }
                match adapter_path {
                    Some(path) => Some(format!("{}/", path.as_str())),
                    None => return Ok(None),
                }
            }
            None => None,
        };

        debug!("root object: {:?}", root_object);

        for (path, interfaces) in devices {
            if let Some(root) = &root_object {
                if !path.starts_with(root) {
                    continue;
                }
            }

            let Some(device_interface) = interfaces.get("org.bluez.Device1") else {
                // Not a device
                continue;
            };

            let addr: &str = device_interface
                .get("Address")
                .and_then(|a| a.downcast_ref().ok())
                .unwrap();
            if addr != mac {
                continue;
            }

            debug!("Found device with path {:?}", path);

            return Ok(Some(Self {
                props: PropertiesProxy::builder(manager_proxy.inner().connection())
                    .destination("org.bluez")
                    .and_then(|x| x.path(path.clone()))
                    .unwrap()
                    .build()
                    .await
                    .error("Failed to create PropertiesProxy")?,
                device: Device1Proxy::builder(manager_proxy.inner().connection())
                    // No caching because https://github.com/greshake/i3status-rust/issues/1565#issuecomment-1379308681
                    .cache_properties(zbus::CacheProperties::No)
                    .path(path.clone())
                    .unwrap()
                    .build()
                    .await
                    .error("Failed to create Device1Proxy")?,
                battery: Battery1Proxy::builder(manager_proxy.inner().connection())
                    .cache_properties(zbus::CacheProperties::No)
                    .path(path)
                    .unwrap()
                    .build()
                    .await
                    .error("Failed to create Battery1Proxy")?,
            }));
        }

        debug!("No device found");
        Ok(None)
    }
}

#[zbus::proxy(interface = "org.bluez.Device1", default_service = "org.bluez")]
trait Device1 {
    fn connect(&self) -> zbus::Result<()>;
    fn disconnect(&self) -> zbus::Result<()>;

    #[zbus(property)]
    fn connected(&self) -> zbus::Result<bool>;

    #[zbus(property)]
    fn name(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn icon(&self) -> zbus::Result<String>;
}

#[zbus::proxy(interface = "org.bluez.Battery1", default_service = "org.bluez")]
trait Battery1 {
    #[zbus(property)]
    fn percentage(&self) -> zbus::Result<u8>;
}

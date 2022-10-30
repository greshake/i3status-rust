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
//! Key | Values | Default
//! ----|--------|--------
//! `mac` | MAC address of the Bluetooth device | **Required**
//! `adapter_mac` | MAC Address of the Bluetooth adapter (in case your device was connected to multiple currently available adapters) | `None`
//! `format` | A string to customise the output of this block. See below for available placeholders. | <code>" $icon $name{ $percentage&vert;} "</code>
//! `disconnected_format` | A string to customise the output of this block. See below for available placeholders. | <code>" $icon{ $name&vert;} "</code>
//!
//! Placeholder  | Value                                                                 | Type   | Unit
//! -------------|-----------------------------------------------------------------------|--------|------
//! `icon`       | Icon based on what type of device is connected                        | Icon   | -
//! `name`       | Device's name                                                         | Text   | -
//! `percentage` | Device's battery level (may be absent if the device is not supported) | Number | %
//! `available`  | Present if the device is available                                    | Flag   | -
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
//! ```
//!
//! # Icons Used
//! - `headphones` for bluetooth devices identifying as "audio-card" or "audio-headset"
//! - `joystick` for bluetooth devices identifying as "input-gaming"
//! - `keyboard` for bluetooth devices identifying as "input-keyboard"
//! - `mouse` for bluetooth devices identifying as "input-mouse"
//! - `bluetooth` for all other devices

use super::prelude::*;
use zbus::fdo::{ObjectManagerProxy, PropertiesProxy};

make_log_macro!(debug, "bluetooth");

#[derive(Deserialize, Debug)]
pub struct Config {
    mac: String,
    #[serde(default)]
    adapter_mac: Option<String>,
    #[serde(default)]
    format: FormatConfig,
    #[serde(default)]
    disconnected_format: FormatConfig,
}

pub async fn run(config: Config, mut api: CommonApi) -> Result<()> {
    let format = config.format.with_default(" $icon $name{ $percentage|} ")?;
    let disconnected_format = config
        .disconnected_format
        .with_default(" $icon{ $name|} ")?;
    let mut widget = Widget::new();

    let mut monitor = DeviceMonitor::new(config.mac, config.adapter_mac).await?;

    loop {
        match &monitor.device {
            // Available
            Some(device) => {
                let connected = device.connected().await?;
                let mut values = map! {
                    "icon" => Value::icon(api.get_icon(device.icon().await?)?),
                    "name" => Value::text(device.name().await?),
                    "available" => Value::flag()
                };
                device
                    .percentage()
                    .await
                    .map(|p| values.insert("percentage".into(), Value::percents(p)));
                if connected {
                    widget.state = State::Good;
                    widget.set_format(format.clone());
                    debug!("Showing device as connected");
                } else {
                    debug!("Showing device as disconnected");
                    widget.set_format(disconnected_format.clone());
                    widget.state = State::Idle;
                }
                widget.set_values(values);

                api.set_widget(&widget).await?;
            }
            // Unavailable
            None => {
                debug!("Showing device as unavailable");
                widget.state = State::Idle;
                widget.set_format(disconnected_format.clone());
                widget.set_values(map!("icon" => Value::icon(api.get_icon("bluetooth")?)));
                api.set_widget(&widget).await?;
            }
        }

        loop {
            select! {
                res = monitor.wait_for_change() => {
                    res?;
                    break;
                },
                event = api.event() => if let Click(click) = event {
                    if click.button == MouseButton::Right {
                        if let Some(dev) = &monitor.device {
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

#[derive(Clone)]
struct Device {
    props: PropertiesProxy<'static>,
    device: Device1Proxy<'static>,
    battery: Battery1Proxy<'static>,
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
                let mut interface_removed = self
                    .manager_proxy
                    .receive_interfaces_removed()
                    .await
                    .error("Failed to monitor interfaces")?;
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
                        Some(event) = interface_removed.next() => {
                            let args = event.args().error("Failed to get the args")?;
                            if args.object_path() == device.device.path() {
                                self.device = None;
                                debug!("Device is no longer available");
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
        let root_object: String = match adapter_mac {
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

        debug!("root object: {:?}", root_object);

        // Iterate over all devices
        let devices = manager_proxy
            .get_managed_objects()
            .await
            .error("Failed to get the list of devices")?;

        debug!("all managed devices: {:?}", devices);

        for (path, interfaces) in devices {
            if !path.starts_with(&format!("{root_object}/")) {
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

            debug!("Found device with path {:?}", path);

            return Ok(Some(Self {
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
                battery: Battery1Proxy::builder(manager_proxy.connection())
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
        self.device.name().await.error("Failed to get name")
    }

    async fn connected(&self) -> Result<bool> {
        self.device
            .connected()
            .await
            .error("Failed to get connected state")
    }

    async fn percentage(&self) -> Option<u8> {
        self.battery.percentage().await.ok()
    }
}

#[zbus::dbus_proxy(interface = "org.bluez.Device1", default_service = "org.bluez")]
trait Device1 {
    fn connect(&self) -> zbus::Result<()>;
    fn disconnect(&self) -> zbus::Result<()>;

    #[dbus_proxy(property)]
    fn connected(&self) -> zbus::Result<bool>;

    #[dbus_proxy(property)]
    fn name(&self) -> zbus::Result<String>;

    #[dbus_proxy(property)]
    fn icon(&self) -> zbus::Result<String>;
}

#[zbus::dbus_proxy(interface = "org.bluez.Battery1", default_service = "org.bluez")]
trait Battery1 {
    #[dbus_proxy(property)]
    fn percentage(&self) -> zbus::Result<u8>;
}

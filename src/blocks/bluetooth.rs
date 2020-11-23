use serde_derive::Deserialize;
use std::collections::BTreeMap;
use std::thread;
use std::time::Instant;

use crossbeam_channel::Sender;
use dbus::ffidisp::stdintf::org_freedesktop_dbus::{ObjectManager, Properties};

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::Config;
use crate::errors::*;
use crate::input::{I3BarEvent, MouseButton};
use crate::scheduler::Task;
use crate::util::pseudo_uuid;
use crate::widget::{I3BarWidget, State};
use crate::widgets::button::ButtonWidget;

pub struct BluetoothDevice {
    pub path: String,
    pub icon: Option<String>,
    pub label: String,
    con: dbus::ffidisp::Connection,
}

impl BluetoothDevice {
    pub fn new(mac: String, label: Option<String>) -> Result<Self> {
        let con = dbus::ffidisp::Connection::get_private(dbus::ffidisp::BusType::System)
            .block_error("bluetooth", "Failed to establish D-Bus connection.")?;

        // Bluez does not provide a convenient way to, say, list devices, so we
        // have to employ a rather verbose workaround.

        let objects = con
            .with_path("org.bluez", "/", 1000)
            .get_managed_objects()
            .block_error("bluetooth", "Failed to get managed objects from org.bluez.")?;

        let devices: Vec<(dbus::Path, String)> = objects
            .into_iter()
            .filter(|(_, interfaces)| interfaces.contains_key("org.bluez.Device1"))
            .map(|(path, interfaces)| {
                let props = interfaces.get("org.bluez.Device1").unwrap();
                // This could be made safer; however, this is the documented
                // D-Bus API format, so it's not a terrible idea to panic if it
                // is violated.
                let address: String = props
                    .get("Address")
                    .unwrap()
                    .0
                    .as_str()
                    .unwrap()
                    .to_string();
                (path, address)
            })
            .collect();

        // If we need to suppress errors from missing devices, this is the place
        // to do it. We could also pick the "default" device here, although that
        // does not make much sense to me in the context of Bluetooth.

        let path = devices
            .into_iter()
            .filter(|(_, address)| address == &mac)
            .map(|(path, _)| path)
            .next()
            .block_error(
                "bluetooth",
                "Failed find a device with matching MAC address.",
            )?
            .to_string();

        // Swallow errors, since this is optional.
        let icon: Option<String> = con
            .with_path("org.bluez", &path, 1000)
            .get("org.bluez.Device1", "Icon")
            .ok();

        Ok(BluetoothDevice {
            path,
            icon,
            label: label.unwrap_or_else(|| "".to_string()),
            con,
        })
    }

    pub fn battery(&self) -> Option<u8> {
        // Swallow errors here; not all devices implement this API.
        self.con
            .with_path("org.bluez", &self.path, 1000)
            .get("org.bluez.Battery1", "Percentage")
            .ok()
    }

    pub fn connected(&self) -> bool {
        self.con
            .with_path("org.bluez", &self.path, 1000)
            .get("org.bluez.Device1", "Connected")
            // In the case that the D-Bus interface missing or responds
            // incorrectly, it seems reasonable to treat the device as "down"
            // instead of nuking the bar. This matches the behaviour elsewhere.
            .unwrap_or(false)
    }

    pub fn toggle(&self) -> Result<()> {
        let method = if self.connected() {
            "Disconnect"
        } else {
            "Connect"
        };
        let msg =
            dbus::Message::new_method_call("org.bluez", &self.path, "org.bluez.Device1", method)
                .block_error("bluetooth", "Failed to build D-Bus method.")?;

        // Swallow errors rather than nuke the bar.
        let _ = self.con.send(msg);
        Ok(())
    }

    /// Monitor Bluetooth property changes in a separate thread and send updates
    /// via the `update_request` channel.
    pub fn monitor(&self, id: String, update_request: Sender<Task>) {
        let path = self.path.clone();
        thread::Builder::new()
            .name("bluetooth".into())
            .spawn(move || {
                let con = dbus::ffidisp::Connection::get_private(dbus::ffidisp::BusType::System)
                    .expect("Failed to establish D-Bus connection.");
                let rule = format!(
                    "type='signal',\
                 path='{}',\
                 interface='org.freedesktop.DBus.Properties',\
                 member='PropertiesChanged'",
                    path
                );

                // Skip the NameAcquired event.
                con.incoming(10_000).next();

                con.add_match(&rule)
                    .expect("Failed to add D-Bus match rule.");

                loop {
                    if con.incoming(10_000).next().is_some() {
                        update_request
                            .send(Task {
                                id: id.clone(),
                                update_time: Instant::now(),
                            })
                            .unwrap();
                    }
                }
            })
            .unwrap();
    }
}

pub struct Bluetooth {
    id: String,
    output: ButtonWidget,
    device: BluetoothDevice,
    hide_disconnected: bool,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct BluetoothConfig {
    pub mac: String,
    pub label: Option<String>,
    #[serde(default = "BluetoothConfig::default_hide_disconnected")]
    pub hide_disconnected: bool,
    #[serde(default = "BluetoothConfig::default_color_overrides")]
    pub color_overrides: Option<BTreeMap<String, String>>,
}

impl BluetoothConfig {
    fn default_hide_disconnected() -> bool {
        false
    }

    fn default_color_overrides() -> Option<BTreeMap<String, String>> {
        None
    }
}

impl ConfigBlock for Bluetooth {
    type Config = BluetoothConfig;

    fn new(block_config: Self::Config, config: Config, send: Sender<Task>) -> Result<Self> {
        let id: String = pseudo_uuid();
        let device = BluetoothDevice::new(block_config.mac, block_config.label)?;
        device.monitor(id.clone(), send);

        Ok(Bluetooth {
            id: id.clone(),
            output: ButtonWidget::new(config, &id).with_icon(match device.icon {
                Some(ref icon) if icon == "audio-card" => "headphones",
                Some(ref icon) if icon == "input-gaming" => "joystick",
                Some(ref icon) if icon == "input-keyboard" => "keyboard",
                Some(ref icon) if icon == "input-mouse" => "mouse",
                _ => "bluetooth",
            }),
            device,
            hide_disconnected: block_config.hide_disconnected,
        })
    }
}

impl Block for Bluetooth {
    fn id(&self) -> &str {
        &self.id
    }

    fn update(&mut self) -> Result<Option<Update>> {
        let connected = self.device.connected();
        self.output.set_text(self.device.label.to_string());
        self.output
            .set_state(if connected { State::Good } else { State::Idle });

        // Use battery info, when available.
        if let Some(value) = self.device.battery() {
            self.output.set_state(match value {
                0..=15 => State::Critical,
                16..=30 => State::Warning,
                31..=60 => State::Info,
                61..=100 => State::Good,
                _ => State::Warning,
            });
            self.output
                .set_text(format!("{} {}%", self.device.label, value));
        }

        Ok(None)
    }

    fn click(&mut self, event: &I3BarEvent) -> Result<()> {
        if let Some(ref name) = event.name {
            if name.as_str() == self.id {
                if let MouseButton::Right = event.button {
                    self.device.toggle()?;
                }
            }
        }
        Ok(())
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        if !self.device.connected() && self.hide_disconnected {
            vec![]
        } else {
            vec![&self.output]
        }
    }
}

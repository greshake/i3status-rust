use serde_derive::Deserialize;

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use dbus::{
    arg::RefArg,
    ffidisp::stdintf::org_freedesktop_dbus::{ObjectManager, Properties},
    message::SignalArgs,
};

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::SharedConfig;
use crate::errors::*;
use crate::formatting::value::Value;
use crate::formatting::FormatTemplate;
use crate::protocol::i3bar_event::{I3BarEvent, MouseButton};
use crate::scheduler::Task;
use crate::widgets::text::TextWidget;
use crate::widgets::{I3BarWidget, State};

pub struct BluetoothDevice {
    pub path: String,
    pub icon: Option<String>,
    pub label: String,
    con: dbus::ffidisp::Connection,
    available: Arc<Mutex<bool>>,
}

impl BluetoothDevice {
    pub fn new(mac: String, controller_id: String, label: Option<String>) -> Result<Self> {
        let con = dbus::ffidisp::Connection::get_private(dbus::ffidisp::BusType::System)
            .block_error("bluetooth", "Failed to establish D-Bus connection.")?;

        // Bluez does not provide a convenient way to list devices, so we
        // have to employ a rather verbose workaround.
        let objects = con
            .with_path("org.bluez", "/", 1000)
            .get_managed_objects()
            .block_error("bluetooth", "Failed to get managed objects from org.bluez.")?;

        // If we need to suppress errors from missing devices, this is the place
        // to do it. We could also pick the "default" device here, although that
        // does not make much sense to me in the context of Bluetooth.
        let mut initial_available = false;
        let auto_path = objects
            .into_iter()
            .filter(|(_, interfaces)| interfaces.contains_key("org.bluez.Device1"))
            .map(|(path, interfaces)| {
                let props = interfaces.get("org.bluez.Device1").unwrap();
                // This could be made safer; however this is the documented
                // D-Bus API format, so it's not a terrible idea to panic if it
                // is violated.
                let address: String = props
                    .get("Address")
                    .unwrap()
                    .0
                    .as_str()
                    .unwrap()
                    .to_string();
                let adapter: String = props
                    .get("Adapter")
                    .unwrap()
                    .0
                    .as_str()
                    .unwrap()
                    .to_string();
                (path, adapter, address)
            })
            .filter(|(_, _, address)| address == &mac)
            .filter(|(_, adapter, _)| adapter.ends_with(&controller_id))
            .map(|(path, _, _)| path)
            .next();
        let path = if let Some(p) = auto_path {
            initial_available = true;
            p
        } else {
            // TODO: possible not to hardcode device?
            dbus::strings::Path::new(format!(
                "/org/bluez/{}/dev_{}",
                controller_id,
                mac.replace(':', "_")
            ))
            .unwrap()
        }
        .to_string();

        // Swallow errors, since this is optional.
        let icon: Option<String> = con
            .with_path("org.bluez", &path, 1000)
            .get("org.bluez.Device1", "Icon")
            .ok();

        // TODO: revisit this lint
        #[allow(clippy::mutex_atomic)]
        let available = Arc::new(Mutex::new(initial_available));

        Ok(BluetoothDevice {
            path,
            icon,
            label: label.unwrap_or_else(|| "".to_string()),
            con,
            available,
        })
    }

    pub fn battery(&self) -> Option<u8> {
        // Swallow errors here; not all devices implement this API.
        self.con
            .with_path("org.bluez", &self.path, 1000)
            .get("org.bluez.Battery1", "Percentage")
            .ok()
    }

    pub fn icon(&self) -> Option<String> {
        self.con
            .with_path("org.bluez", &self.path, 1000)
            .get("org.bluez.Device1", "Icon")
            .ok()
    }

    pub fn available(&self) -> Result<bool> {
        Ok(*self
            .available
            .lock()
            .block_error("bluetooth", "failed to acquire lock for `available`")?)
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
        // TODO: power on adapter if it's off
        // i.e. busctl --system set-property org.bluez /org/bluez/hci0 org.bluez.Adapter1 Powered b true
        let method = if self.connected() {
            "Disconnect"
        } else {
            "Connect"
        };
        let msg =
            dbus::Message::new_method_call("org.bluez", &self.path, "org.bluez.Device1", method)
                .block_error("bluetooth", "Failed to build D-Bus method.")?;

        let _ = self.con.send(msg);
        Ok(())
    }

    /// Monitor Bluetooth property changes in a separate thread and send updates
    /// via the `update_request` channel.
    pub fn monitor(&self, id: usize, update_request: Sender<Task>) {
        let path_copy1 = self.path.clone();
        let path_copy2 = self.path.clone();
        let avail_copy1 = self.available.clone();
        let avail_copy2 = self.available.clone();
        let update_request_copy1 = update_request.clone();
        let update_request_copy2 = update_request.clone();
        let update_request_copy3 = update_request;

        thread::Builder::new().name("bluetooth".into()).spawn(move || {
            let c = dbus::blocking::Connection::new_system().unwrap();
            use dbus::ffidisp::stdintf::org_freedesktop_dbus::ObjectManagerInterfacesAdded as IA;
            let ma = IA::match_rule(Some(&"org.bluez".into()), None).static_clone();
            c.add_match(ma, move |ia: IA, _, _| {
                if ia.object == path_copy1.clone().into() {
                    let mut avail = avail_copy1.lock().unwrap();
                    *avail = true;
                    update_request_copy1
                        .send(Task {
                            id,
                            update_time: Instant::now(),
                        })
                        .unwrap();
                }
                true
            })
            .unwrap();

            use dbus::ffidisp::stdintf::org_freedesktop_dbus::ObjectManagerInterfacesRemoved as IR;
            let mr = IR::match_rule(Some(&"org.bluez".into()), None).static_clone();
            c.add_match(mr, move |ir: IR, _, _| {
                if ir.object == path_copy2.clone().into() {
                    let mut avail = avail_copy2.lock().unwrap();
                    *avail = false;
                    update_request_copy2
                        .send(Task {
                            id,
                            update_time: Instant::now(),
                        })
                        .unwrap();
                }
                true
            })
            .unwrap();

            use dbus::ffidisp::stdintf::org_freedesktop_dbus::PropertiesPropertiesChanged as PPC;
            let mr = PPC::match_rule(Some(&"org.bluez".into()), None).static_clone();
            // TODO: get updated values from the signal message
            c.add_match(mr, move |_ppc: PPC, _, _| {
                update_request_copy3
                    .send(Task {
                        id,
                        update_time: Instant::now(),
                    })
                    .unwrap();
                true
            })
            .unwrap();

            loop {
                c.process(Duration::from_millis(1000)).unwrap();
            }
        }).unwrap();
    }
}

pub struct Bluetooth {
    id: usize,
    output: TextWidget,
    device: BluetoothDevice,
    hide_disconnected: bool,
    format: FormatTemplate,
    format_disconnected: FormatTemplate,
    format_unavailable: FormatTemplate,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct BluetoothConfig {
    pub mac: String,
    #[serde(default = "default_controller")]
    pub controller_id: String,
    #[serde(default)]
    pub hide_disconnected: bool,
    #[serde(default)]
    pub format: FormatTemplate,
    #[serde(default)]
    pub format_disconnected: FormatTemplate,
    #[serde(default)]
    pub format_unavailable: FormatTemplate,
    //DEPRECATED, TODO: REMOVE
    pub label: Option<String>,
}

fn default_controller() -> String {
    // If there are multiple controllers, then the wrong one might be selected on updates.
    // Avoid this by allowing the controller to be specified.
    // This also applies in the case where the bluetooth module is disabled on startup,
    // and we set a manual fallback path. There's no way to know the controller id beforehand,
    // so we default to hci0, but this might not always be the desired controller.
    String::from("hci0")
}

impl ConfigBlock for Bluetooth {
    type Config = BluetoothConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        shared_config: SharedConfig,
        send: Sender<Task>,
    ) -> Result<Self> {
        let device = BluetoothDevice::new(
            block_config.mac,
            block_config.controller_id,
            block_config.label,
        )?;
        device.monitor(id, send);

        Ok(Bluetooth {
            id,
            output: TextWidget::new(id, 0, shared_config).with_icon(match device.icon {
                Some(ref icon) if icon == "audio-card" => "headphones",
                Some(ref icon) if icon == "input-gaming" => "joystick",
                Some(ref icon) if icon == "input-keyboard" => "keyboard",
                Some(ref icon) if icon == "input-mouse" => "mouse",
                _ => "bluetooth",
            })?,
            device,
            hide_disconnected: block_config.hide_disconnected,
            format: block_config.format.with_default("{label} {percentage}")?,
            format_disconnected: block_config.format_disconnected.with_default("{label}")?,
            format_unavailable: block_config.format_unavailable.with_default("{label} x")?,
        })
    }
}

impl Block for Bluetooth {
    fn id(&self) -> usize {
        self.id
    }

    fn update(&mut self) -> Result<Option<Update>> {
        if self.device.available()? {
            let values = map!(
                "label" => Value::from_string(self.device.label.clone()),
                "percentage" => Value::from_integer(self.device.battery().unwrap_or(0) as i64).percents(),
            );

            let connected = self.device.connected();
            self.output.set_text(self.device.label.to_string());
            self.output
                .set_state(if connected { State::Good } else { State::Idle });

            self.output.set_icon(match self.device.icon() {
                Some(ref icon) if icon == "audio-card" => "headphones",
                Some(ref icon) if icon == "input-gaming" => "joystick",
                Some(ref icon) if icon == "input-keyboard" => "keyboard",
                Some(ref icon) if icon == "input-mouse" => "mouse",
                _ => "bluetooth",
            })?;

            // Use battery info, when available.
            if let Some(value) = self.device.battery() {
                self.output.set_state(match value {
                    0..=15 => State::Critical,
                    16..=30 => State::Warning,
                    31..=60 => State::Info,
                    61..=100 => State::Good,
                    _ => State::Warning,
                });
            }
            if connected {
                self.output.set_texts(self.format.render(&values)?);
            } else {
                self.output
                    .set_texts(self.format_disconnected.render(&values)?);
            }
        } else {
            let values = map!(
                "label" => Value::from_string(self.device.label.clone()),
                "percentage" => Value::from_string("".into()),
            );
            self.output.set_state(State::Idle);
            self.output
                .set_texts(self.format_unavailable.render(&values)?);
        }

        Ok(None)
    }

    fn click(&mut self, event: &I3BarEvent) -> Result<()> {
        if let MouseButton::Right = event.button {
            self.device.toggle()?;
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

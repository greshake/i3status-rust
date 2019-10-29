use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use dbus;
use dbus::stdintf::org_freedesktop_dbus::Properties;
use dbus::{Message, MsgHandlerResult, MsgHandlerType};
use uuid::Uuid;

use crate::blocks::{Block, ConfigBlock};
use crate::config::Config;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::scheduler::Task;
use crate::widget::I3BarWidget;
use crate::widgets::text::TextWidget;

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "lowercase")]
pub enum KeyboardLayoutDriver {
    SetXkbMap,
    LocaleBus,
    KbddBus,
}

impl Default for KeyboardLayoutDriver {
    fn default() -> Self {
        KeyboardLayoutDriver::SetXkbMap
    }
}

pub trait KeyboardLayoutMonitor {
    /// Retrieve the current keyboard layout.
    fn keyboard_layout(&self) -> Result<String>;

    /// Specify that the monitor does not send update requests and must be
    /// polled manually.
    fn must_poll(&self) -> bool;

    /// Monitor layout changes and send updates via the `update_request`
    /// channel. By default, this method does nothing.
    fn monitor(&self, _id: String, _update_request: Sender<Task>) {}
}

pub struct SetXkbMap;

impl SetXkbMap {
    pub fn new() -> Result<SetXkbMap> {
        // TODO: Check that setxkbmap is available.
        Ok(SetXkbMap)
    }
}

fn setxkbmap_layouts() -> Result<String> {
    let output = Command::new("setxkbmap")
        .args(&["-query"])
        .output()
        .block_error("keyboard_layout", "Failed to exectute setxkbmap.")
        .and_then(|raw| {
            String::from_utf8(raw.stdout).block_error("keyboard_layout", "Non-UTF8 input.")
        })?;

    // Find the "layout:    xxxx" entry.
    let layout = output
        .split('\n')
        .filter(|line| line.starts_with("layout"))
        .next()
        .ok_or_else(|| {
            BlockError(
                "keyboard_layout".to_string(),
                "Could not find the layout entry from setxkbmap.".to_string(),
            )
        })?
        .split(char::is_whitespace)
        .last();

    match layout {
        Some(layout) => Ok(layout.to_string()),
        None => Err(BlockError(
            "keyboard_layout".to_string(),
            "Could not read the layout entry from setxkbmap.".to_string(),
        )),
    }
}

impl KeyboardLayoutMonitor for SetXkbMap {
    fn keyboard_layout(&self) -> Result<String> {
        return setxkbmap_layouts();
    }

    fn must_poll(&self) -> bool {
        true
    }
}

pub struct LocaleBus {
    con: dbus::Connection,
}

impl LocaleBus {
    pub fn new() -> Result<Self> {
        let con = dbus::Connection::get_private(dbus::BusType::System)
            .block_error("locale", "Failed to establish D-Bus connection.")?;

        Ok(LocaleBus { con: con })
    }
}

impl KeyboardLayoutMonitor for LocaleBus {
    fn keyboard_layout(&self) -> Result<String> {
        let layout: String = self
            .con
            .with_path("org.freedesktop.locale1", "/org/freedesktop/locale1", 1000)
            .get("org.freedesktop.locale1", "X11Layout")
            .block_error("locale", "Failed to get X11Layout property.")?;

        Ok(layout)
    }

    fn must_poll(&self) -> bool {
        false
    }

    /// Monitor Locale property changes in a separate thread and send updates
    /// via the `update_request` channel.
    fn monitor(&self, id: String, update_request: Sender<Task>) {
        thread::spawn(move || {
            let con = dbus::Connection::get_private(dbus::BusType::System)
                .expect("Failed to establish D-Bus connection.");
            let rule = "type='signal',\
                        path='/org/freedesktop/locale1',\
                        interface='org.freedesktop.DBus.Properties',\
                        member='PropertiesChanged'";

            // Skip the NameAcquired event.
            con.incoming(10_000).next();

            con.add_match(&rule)
                .expect("Failed to add D-Bus match rule.");

            loop {
                // TODO: This actually seems to trigger twice for each localectl
                // change.
                if con.incoming(10_000).next().is_some() {
                    update_request
                        .send(Task {
                            id: id.clone(),
                            update_time: Instant::now(),
                        })
                        .unwrap();
                }
            }
        });
    }
}

// KbdDaemonBus - use this option if you have kbdd running (https://github.com/qnikst/kbdd,
// also available in AUR and Debian) running, which enables per window keyboard layout,
// really handy for dual-language typists who often change window focus
pub struct KbdDaemonBus {
    // extracted from kbdd dbus message
    kbdd_layout_id: Arc<Mutex<u32>>,
}

impl KbdDaemonBus {
    pub fn new() -> Result<Self> {
        Command::new("setxkbmap")
            .arg("-version")
            .stdout(Stdio::piped())
            .spawn()
            .block_error("kbddaemonbus", "setxkbmap not found")?;

        // also verifies that kbdd daemon is registered in dbus
        let layout_id = KbdDaemonBus::get_initial_layout_id()?;

        Ok(KbdDaemonBus {
            kbdd_layout_id: Arc::new(Mutex::new(layout_id)),
        })
    }

    fn get_initial_layout_id() -> Result<u32> {
        let c = dbus::Connection::get_private(dbus::BusType::Session)
            .block_error("kbddaemonbus", "can't connect to dbus")?;

        let send_msg = Message::new_method_call(
            "ru.gentoo.KbddService",
            "/ru/gentoo/KbddService",
            "ru.gentoo.kbdd",
            "getCurrentLayout",
        )
        .block_error("kbddaemonbus", "Create get-layout-id message failure")?;

        let repl_msg = c
            .send_with_reply_and_block(send_msg, 5000)
            .block_error("kbddaemonbus", "Is kbdd running?")?;

        let current_layout_id: u32 = repl_msg
            .get1()
            .ok_or("")
            .block_error("kbddaemonbus", "dbus kbdd response error")?;

        Ok(current_layout_id)
    }
}

impl KeyboardLayoutMonitor for KbdDaemonBus {
    fn keyboard_layout(&self) -> Result<String> {
        let layouts_str = setxkbmap_layouts()?;
        let idx = *self.kbdd_layout_id.lock().unwrap();

        let split = layouts_str.split(",").nth(idx as usize);

        match split {
            //sometimes (after keyboard attach/detach) setxkbmap reports variant in the layout line,
            //e.g. 'layout:     us,bg:bas_phonetic,' instead of printing it in its own line,
            //there is no need to waste space for it, because in most cases there will be only one
            //variant per layout. TODO - add configuration option to show the variant?!
            Some(s) => Ok(s.split(":").nth(0).unwrap().to_string()),

            //'None' may happen only if keyboard map is being toggled (by window focus or keyboard)
            //and keyboard layout replaced (by calling setxkmap) at almost the same time,
            //frankly I can't reproduce this without thread sleep in monitor function, so instead
            //of block_error I think showing all layouts will be better until the next
            //toggling happens
            None => Ok(layouts_str),
        }
    }

    fn must_poll(&self) -> bool {
        false
    }

    // Monitor KbdDaemon 'layoutChanged' property in a separate thread and send updates
    // via the `update_request` channel.
    fn monitor(&self, id: String, update_request: Sender<Task>) {
        let arc = Arc::clone(&self.kbdd_layout_id);
        thread::spawn(move || {
            let c = dbus::Connection::get_private(dbus::BusType::Session).unwrap();
            c.add_match(
                "interface='ru.gentoo.kbdd',\
                 member='layoutChanged',\
                 path='/ru/gentoo/KbddService'",
            )
            .expect("Failed to add D-Bus match rule, is kbdd started?");

            // skip NameAcquired
            c.incoming(10_000).next();

            c.add_handler(KbddMessageHandler(arc));
            loop {
                for ci in c.iter(100_000) {
                    if let dbus::ConnectionItem::Signal(_) = ci {
                        update_request
                            .send(Task {
                                id: id.clone(),
                                update_time: Instant::now(),
                            })
                            .unwrap();
                    }
                }
            }
        });
    }
}

struct KbddMessageHandler(Arc<Mutex<u32>>);

impl dbus::MsgHandler for KbddMessageHandler {
    fn handler_type(&self) -> MsgHandlerType {
        return dbus::MsgHandlerType::MsgType(dbus::MessageType::Signal);
    }

    fn handle_msg(&mut self, msg: &Message) -> Option<MsgHandlerResult> {
        let layout: Option<u32> = msg.get1();
        if let Some(idx) = layout {
            let mut val = self.0.lock().unwrap();
            *val = idx;
        }
        //handled=false - because we still need to call update_request.send in monitor
        Some(MsgHandlerResult {
            handled: false,
            done: false,
            reply: vec![],
        })
    }
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct KeyboardLayoutConfig {
    driver: KeyboardLayoutDriver,
    #[serde(
        default = "KeyboardLayoutConfig::default_interval",
        deserialize_with = "deserialize_duration"
    )]
    interval: Duration,
}

impl KeyboardLayoutConfig {
    fn default_interval() -> Duration {
        Duration::from_secs(60)
    }
}

pub struct KeyboardLayout {
    id: String,
    output: TextWidget,
    monitor: Box<dyn KeyboardLayoutMonitor>,
    update_interval: Option<Duration>,
}

impl ConfigBlock for KeyboardLayout {
    type Config = KeyboardLayoutConfig;

    fn new(block_config: Self::Config, config: Config, send: Sender<Task>) -> Result<Self> {
        let id: String = Uuid::new_v4().simple().to_string();
        let monitor: Box<dyn KeyboardLayoutMonitor> = match block_config.driver {
            KeyboardLayoutDriver::SetXkbMap => Box::new(SetXkbMap::new()?),
            KeyboardLayoutDriver::LocaleBus => {
                let monitor = LocaleBus::new()?;
                monitor.monitor(id.clone(), send);
                Box::new(monitor)
            }
            KeyboardLayoutDriver::KbddBus => {
                let monitor = KbdDaemonBus::new()?;
                monitor.monitor(id.clone(), send);
                Box::new(monitor)
            }
        };
        let update_interval = match monitor.must_poll() {
            true => Some(block_config.interval),
            false => None,
        };
        Ok(KeyboardLayout {
            id: id,
            output: TextWidget::new(config),
            monitor,
            update_interval,
        })
    }
}

impl Block for KeyboardLayout {
    fn id(&self) -> &str {
        &self.id
    }

    fn update(&mut self) -> Result<Option<Duration>> {
        self.output.set_text(self.monitor.keyboard_layout()?);
        Ok(self.update_interval)
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.output]
    }
}

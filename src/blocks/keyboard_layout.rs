use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use dbus;
use dbus::{Message, MsgHandlerResult, MsgHandlerType};
use dbus::stdintf::org_freedesktop_dbus::Properties;
use regex::Regex;
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

impl KeyboardLayoutMonitor for SetXkbMap {
    fn keyboard_layout(&self) -> Result<String> {
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
                    update_request.send(Task {
                        id: id.clone(),
                        update_time: Instant::now(),
                    }).unwrap();
                }
            }
        });
    }
}

// KbdDaemonBus - use this option if you have kbdd running (https://github.com/qnikst/kbdd,
// also available in AUR and Debian) running, which enables per window keyboard layout,
// really handy for dual-language typists who often change window focus
pub struct KbdDaemonBus {
    re_layout: Regex,
    re_leds: Regex,

    // indicates if xkblayout-state is installed
    has_xkblayout: bool,
    // indicates if setxkbmap && xset are installed
    has_setxkbmap: bool,
    // extracted from dbus message
    kbdd_layout: Arc<Mutex<Option<String>>>,
}

impl KbdDaemonBus {
    pub fn new() -> Result<Self> {
        // also verifies that kbdd daemon is registered in dbus
        let layout = KbdDaemonBus::get_initial_layout()?;
        Ok(KbdDaemonBus {
            has_xkblayout: Command::new("xkblayout-state")
                .stdout(Stdio::piped())
                .args(&["print", "%s"])
                .spawn()
                .is_ok(),

            has_setxkbmap: Command::new("setxkbmap")
                .arg("-version")
                .stdout(Stdio::piped())
                .spawn()
                .is_ok()
                && Command::new("xset")
                .arg("-version")
                .stdout(Stdio::piped())
                .spawn()
                .is_ok(),

            // example: layout:     us,bg:bas_phonetic
            re_layout: Regex::new(r".*layout:.[ ]+(([a-zA-Z,:()_-]+,?)+).*").unwrap(),
            // example: auto repeat:  on    key click percent:  0    LED mask:  00001000
            re_leds: Regex::new(r".*LED mask:[ ]+[0-9]{4}([01])[0-9]{3}.*").unwrap(),

            kbdd_layout: Arc::new(Mutex::new(Some(layout))),
        })
    }

    fn get_initial_layout() -> Result<String> {
        let c = dbus::Connection::get_private(dbus::BusType::Session)
            .block_error("kbddaemonbus", "can't connect to dbus")?;

        let send_msg = Message::new_method_call("ru.gentoo.KbddService",
                                                "/ru/gentoo/KbddService",
                                                "ru.gentoo.kbdd",
                                                "getCurrentLayout")
            .block_error("kbddaemonbus", "Create get-layout-id message failure")?;

        let repl_msg = c.send_with_reply_and_block(send_msg, 5000)
            .block_error("kbddaemonbus", "Is kbdd running?")?;

        let current_layout_id: u32 = repl_msg.get1().ok_or("")
            .block_error("kbddaemonbus", "dbus kbdd response error")?;

        let send_msg = Message::new_method_call(
            "ru.gentoo.KbddService",
            "/ru/gentoo/KbddService",
            "ru.gentoo.kbdd",
            "getLayoutName")
            .block_error("kbddaemonbus", "Create get-layout-name message failure")?
            .append(current_layout_id);

        let repl_msg = c.send_with_reply_and_block(send_msg, 5000)
            .block_error("kbddaemonbus", "dbus send message error")?;

        let layout_name: &str = repl_msg.get1().ok_or("")
            .block_error("kbddaemonbus", "Error obtaining current layout name")?;

        Ok(layout_name.split(char::is_whitespace).nth(0).unwrap().to_string())
    }
}

impl KbdDaemonBus {
    // load current keyboard layout using xkblayout-state
    fn xkblayout_state_layout(&self) -> Result<String> {
        let output = Command::new("xkblayout-state")
            .args(&["print", "%s"])
            .output()
            .block_error("kbddaemonbus", "Failed to execute xkblayout-state")?
            .stdout;
        let result_str = String::from_utf8(output)
            .block_error("kbddaemonbus", "Non-UTF8 input from xkblayout-state")?;
        Ok(result_str)
    }

    // Load current keyboard layout using setxkbmap and xset,
    // the former shows enabled keyboard layouts, and the latter is
    // used to obtain the current one.
    // This is going to work for 2 layouts only, maybe if there are more
    // it should fallback to the last variant (self.has_setxkbmap=false) and
    // use kbdd bus value (we already have it, but it's long literal, e.g. 'English',
    // which I don't really like, since it consumes a lot of space)
    fn setxkmbmap_layout(&self) -> Result<String> {
        //find enabled layouts
        let output = Command::new("setxkbmap")
            .arg("-query")
            .output()
            .block_error("kbddaemonbus", "Failed to execute setxkbmap")?
            .stdout;

        let result_str = String::from_utf8(output)
            .block_error("kbddaemonbus", "Non-UTF8 input from setxkbmap")?;

        let layout_match = result_str.lines()
            .find_map(|line| {
                self.re_layout.captures(line)
            }).block_error("kbddaemonbus", "Failed to find current layouts")?
            .get(1).unwrap(); //we are sure that it's there because captures would have failed
        let layouts: Vec<&str> = layout_match.as_str().split(",").map(|s| s).collect();

        // check the current layout by reading LED mask in xset output,
        // 0000000 for the first, 0000100 for second,
        // if there are more enabled we will show the second one :(
        let output = Command::new("xset")
            .arg("-q")
            .output()
            .block_error("kbddaemonbus", "Failed to execute xset")?
            .stdout;

        let result_str = String::from_utf8(output)
            .block_error("kbddaemonbus", "Non-UTF8 input from xset")?;

        let leds_indicator = result_str.lines()
            .find_map(|line| {
                self.re_leds.captures(line)
            }).block_error("kbddaemonbus", "Failed to parse xset output")?
            .get(1).unwrap()
            .as_str()
            .parse::<usize>()
            .block_error("kbddaemonbus", "Failed to parse xset leds output")?;

        let layout: &&str = layouts.get(leds_indicator).ok_or("no item?")
            .block_error("kbddaemonbus", "Failed to find current layout")?;

        Ok(layout.split(":").nth(0).unwrap().to_string()) //remove variant if present, e.g. bg:pas_phonetic
    }
}

impl KeyboardLayoutMonitor for KbdDaemonBus {
    fn keyboard_layout(&self) -> Result<String> {
        if self.has_xkblayout {
            self.xkblayout_state_layout()
        } else if self.has_setxkbmap {
            self.setxkmbmap_layout()
        } else {
            match &*self.kbdd_layout.lock().unwrap() {
                Some(s) => Ok((&s).to_string()),
                None => Err(BlockError(
                    "kbddaemonbus".to_string(),
                    "Failed to load layout from kbdd".to_owned())) //should never happen :)
            }
        }
    }

    fn must_poll(&self) -> bool { false }

    // Monitor KbdDaemon property changes in a separate thread and send updates
    // via the `update_request` channel.
    fn monitor(&self, id: String, update_request: Sender<Task>) {
        let arc = Arc::clone(&self.kbdd_layout);
        thread::spawn(move || {
            let c = dbus::Connection::get_private(dbus::BusType::Session).unwrap();
            c.add_match(
                "interface='ru.gentoo.kbdd',\
                member='layoutNameChanged',\
                path='/ru/gentoo/KbddService'",
            ).expect("Failed to add D-Bus match rule, is kbdd started?");

            // skip NameAcquired
            c.incoming(10_000).next();

            c.add_handler(KbddMessageHandler(arc));
            loop {
                for ci in c.iter(100_000) {
                    if let dbus::ConnectionItem::Signal(_) = ci {
                        update_request.send(Task {
                            id: id.clone(),
                            update_time: Instant::now(),
                        }).unwrap();
                    }
                }
            }
        });
    }
}

struct KbddMessageHandler(Arc<Mutex<Option<String>>>);

impl dbus::MsgHandler for KbddMessageHandler {
    fn handler_type(&self) -> MsgHandlerType {
        return dbus::MsgHandlerType::MsgType(dbus::MessageType::Signal);
    }

    fn handle_msg(&mut self, msg: &Message) -> Option<MsgHandlerResult> {
        let mut val = self.0.lock().unwrap();
        let layout: Option<&str> = msg.get1();
        *val = match layout {
            Some(v) => Some(v.split(char::is_whitespace).nth(0).unwrap().to_string()),
            None => None,
        };
        //handled=false - because we still need to call update_request.send in monitor
        Some(MsgHandlerResult { handled: false, done: false, reply: vec![] })
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

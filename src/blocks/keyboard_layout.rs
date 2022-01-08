use std::collections::HashMap;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use dbus::ffidisp::stdintf::org_freedesktop_dbus::Properties;
use dbus::{
    ffidisp::{MsgHandlerResult, MsgHandlerType},
    Message,
};
use serde_derive::Deserialize;
use swayipc::{Connection, Event, EventType, InputChange};

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::SharedConfig;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::formatting::value::Value;
use crate::formatting::FormatTemplate;
use crate::scheduler::Task;
use crate::widgets::text::TextWidget;
use crate::widgets::I3BarWidget;

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "lowercase")]
pub enum KeyboardLayoutDriver {
    SetXkbMap,
    LocaleBus,
    KbddBus,
    XkbSwitch,
    Sway,
}

pub trait KeyboardLayoutMonitor {
    /// Retrieve the current keyboard layout.
    fn keyboard_layout(&self) -> Result<String>;

    /// Retrieve the current keyboard variant.
    fn keyboard_variant(&self) -> Result<String>;

    /// Specify that the monitor does not send update requests and must be
    /// polled manually.
    fn must_poll(&self) -> bool;

    /// Monitor layout changes and send updates via the `update_request`
    /// channel. By default, this method does nothing.
    fn monitor(&self, _id: usize, _update_request: Sender<Task>) {}
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
        .block_error("keyboard_layout", "Failed to execute setxkbmap.")
        .and_then(|raw| {
            String::from_utf8(raw.stdout).block_error("keyboard_layout", "Non-UTF8 input.")
        })?;

    // Find the "layout:    xxxx" entry.
    let layout = output
        .split('\n')
        .find(|line| line.starts_with("layout"))
        .block_error(
            "keyboard_layout",
            "Could not find the layout entry from setxkbmap.",
        )?
        .split(char::is_whitespace)
        .last();

    layout.map(|s| s.to_string()).block_error(
        "keyboard_layout",
        "Could not read the layout entry from setxkbmap.",
    )
}

impl KeyboardLayoutMonitor for SetXkbMap {
    fn keyboard_layout(&self) -> Result<String> {
        setxkbmap_layouts()
    }

    // Not implemented (TODO?)
    fn keyboard_variant(&self) -> Result<String> {
        Ok("N/A".to_string())
    }

    fn must_poll(&self) -> bool {
        true
    }
}

pub struct LocaleBus {
    con: dbus::ffidisp::Connection,
}

impl LocaleBus {
    pub fn new() -> Result<Self> {
        let con = dbus::ffidisp::Connection::get_private(dbus::ffidisp::BusType::System)
            .block_error("locale", "Failed to establish D-Bus connection.")?;

        Ok(LocaleBus { con })
    }
}

impl KeyboardLayoutMonitor for LocaleBus {
    fn keyboard_layout(&self) -> Result<String> {
        self.con
            .with_path("org.freedesktop.locale1", "/org/freedesktop/locale1", 1000)
            .get("org.freedesktop.locale1", "X11Layout")
            .block_error("locale", "Failed to get X11Layout property.")
    }

    fn keyboard_variant(&self) -> Result<String> {
        self.con
            .with_path("org.freedesktop.locale1", "/org/freedesktop/locale1", 1000)
            .get("org.freedesktop.locale1", "X11Variant")
            .block_error("locale", "Failed to get X11Variant property.")
    }

    fn must_poll(&self) -> bool {
        false
    }

    /// Monitor Locale property changes in a separate thread and send updates
    /// via the `update_request` channel.
    // TODO: pull the new value from the PropertiesChanged message instead of making another method call
    fn monitor(&self, id: usize, update_request: Sender<Task>) {
        thread::Builder::new()
            .name("keyboard_layout".into())
            .spawn(move || {
                let con = dbus::ffidisp::Connection::get_private(dbus::ffidisp::BusType::System)
                    .expect("Failed to establish D-Bus connection.");
                let rule = "type='signal',\
                        path='/org/freedesktop/locale1',\
                        interface='org.freedesktop.DBus.Properties',\
                        member='PropertiesChanged'";

                // Skip the NameAcquired event.
                con.incoming(10_000).next();

                con.add_match(rule)
                    .expect("Failed to add D-Bus match rule.");

                loop {
                    // TODO: This actually seems to trigger twice for each localectl
                    // change.
                    if con.incoming(10_000).next().is_some() {
                        update_request
                            .send(Task {
                                id,
                                update_time: Instant::now(),
                            })
                            .unwrap();
                    }
                }
            })
            .unwrap();
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
            .output()
            .block_error("kbddaemonbus", "setxkbmap not found")?;

        // also verifies that kbdd daemon is registered in dbus
        let layout_id = KbdDaemonBus::get_initial_layout_id()?;

        Ok(KbdDaemonBus {
            kbdd_layout_id: Arc::new(Mutex::new(layout_id)),
        })
    }

    fn get_initial_layout_id() -> Result<u32> {
        let c = dbus::ffidisp::Connection::get_private(dbus::ffidisp::BusType::Session)
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

        let split = layouts_str.split(',').nth(idx as usize);

        match split {
            //sometimes (after keyboard attach/detach) setxkbmap reports variant in the layout line,
            //e.g. 'layout:     us,bg:bas_phonetic,' instead of printing it in its own line,
            //there is no need to waste space for it, because in most cases there will be only one
            //variant per layout. TODO - add configuration option to show the variant?!
            Some(s) => Ok(s.split(':').next().unwrap().to_string()),

            //'None' may happen only if keyboard map is being toggled (by window focus or keyboard)
            //and keyboard layout replaced (by calling setxkmap) at almost the same time,
            //frankly I can't reproduce this without thread sleep in monitor function, so instead
            //of block_error I think showing all layouts will be better until the next
            //toggling happens
            None => Ok(layouts_str),
        }
    }

    fn keyboard_variant(&self) -> Result<String> {
        // Not implemented (TODO?)
        Ok("N/A".to_string())
    }

    fn must_poll(&self) -> bool {
        false
    }

    // Monitor KbdDaemon 'layoutChanged' property in a separate thread and send updates
    // via the `update_request` channel.
    fn monitor(&self, id: usize, update_request: Sender<Task>) {
        let arc = Arc::clone(&self.kbdd_layout_id);
        thread::Builder::new()
            .name("keyboard_layout".into())
            .spawn(move || {
                let c = dbus::ffidisp::Connection::get_private(dbus::ffidisp::BusType::Session)
                    .unwrap();
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
                        if let dbus::ffidisp::ConnectionItem::Signal(_) = ci {
                            update_request
                                .send(Task {
                                    id,
                                    update_time: Instant::now(),
                                })
                                .unwrap();
                        }
                    }
                }
            })
            .unwrap();
    }
}

struct KbddMessageHandler(Arc<Mutex<u32>>);

impl dbus::ffidisp::MsgHandler for KbddMessageHandler {
    fn handler_type(&self) -> MsgHandlerType {
        dbus::ffidisp::MsgHandlerType::MsgType(dbus::MessageType::Signal)
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

pub struct Sway {
    sway_kb_layout: Arc<Mutex<String>>,
}

impl Sway {
    pub fn new(sway_kb_identifier: Option<String>) -> Result<Self> {
        let layout = swayipc::Connection::new()
            .unwrap()
            .get_inputs()
            .unwrap()
            .into_iter()
            .find(|input| {
                sway_kb_identifier
                    .as_ref()
                    .map(|s| s == &input.identifier)
                    .unwrap_or(true)
                    && input.input_type == "keyboard"
            })
            .and_then(|input| input.xkb_active_layout_name)
            .block_error("sway", "Failed to get xkb_active_layout_name.")?;

        Ok(Sway {
            sway_kb_layout: Arc::new(Mutex::new(layout)),
        })
    }
}

impl KeyboardLayoutMonitor for Sway {
    fn keyboard_layout(&self) -> Result<String> {
        // Layout is either `layout (varinat)` or `layout`
        let layout = self.sway_kb_layout.lock().unwrap();
        let (layout, _variant) = match layout.find('(') {
            Some(i) => layout.split_at(i),
            None => return Ok(layout.to_string()),
        };
        Ok(layout
            .split_whitespace()
            .next()
            .unwrap_or(layout)
            .to_string())
    }

    fn keyboard_variant(&self) -> Result<String> {
        // Layout is either `layout (variant)` or `layout`
        // Refer to `man xkeyboard-config`
        let layout = self.sway_kb_layout.lock().unwrap();
        let (_layout, variant) = match layout.find('(') {
            Some(i) => layout.split_at(i),
            None => return Ok("N/A".to_string()),
        };
        Ok(variant[1..variant.len() - 1].to_string())
    }

    fn must_poll(&self) -> bool {
        false
    }

    /// Monitor layout changes in a separate thread and send updates
    /// via the `update_request` channel.
    fn monitor(&self, id: usize, update_request: Sender<Task>) {
        let arc = Arc::clone(&self.sway_kb_layout);
        thread::Builder::new()
            .name("keyboard_layout".into())
            .spawn(move || {
                for event in Connection::new()
                    .unwrap()
                    .subscribe(&[EventType::Input])
                    .unwrap()
                {
                    match event.unwrap() {
                        Event::Input(e) => match e.change {
                            InputChange::XkbLayout => {
                                if let Some(name) = e.input.xkb_active_layout_name {
                                    let mut layout = arc.lock().unwrap();
                                    *layout = name;
                                }
                                update_request
                                    .send(Task {
                                        id,
                                        update_time: Instant::now(),
                                    })
                                    .unwrap();
                            }
                            InputChange::XkbKeymap => {
                                if let Some(name) = e.input.xkb_active_layout_name {
                                    let mut layout = arc.lock().unwrap();
                                    *layout = name;
                                }
                                update_request
                                    .send(Task {
                                        id,
                                        update_time: Instant::now(),
                                    })
                                    .unwrap();
                            }
                            _ => {}
                        },
                        _ => unreachable!(),
                    }
                }
            })
            .unwrap();
    }
}

pub struct XkbSwitch;

impl XkbSwitch {
    pub fn new() -> Result<XkbSwitch> {
        Command::new("xkb-switch")
            .output()
            .block_error("keyboard_layout", "Failed to find xkb-switch in PATH")
            .map(|_| XkbSwitch)
    }
}

fn xkb_switch_show_layout_and_variant() -> Result<(String, Option<String>)> {
    // This command should return a string like "layout(variant)" or "layout"
    Command::new("xkb-switch")
        .args(&["-p"])
        .output()
        .block_error("keyboard_layout", "Failed to execute `xkb-switch -p`.")
        .and_then(|raw| {
            String::from_utf8(raw.stdout).block_error("keyboard_layout", "Non-UTF8 input.")
        })
        .and_then(|layout_and_variant| {
            let mut components = layout_and_variant.trim_end().split('(');

            let layout = components
                .next()
                .ok_or("")
                .block_error("keyboard_layout", "Unable to find keyboard layout")?
                .to_string();

            let variant = components
                .last()
                // Remove the trailing parenthesis ")"
                .map(|variant_str| variant_str.split_at(variant_str.len() - 1).0.to_string());

            Ok((layout, variant))
        })
}

impl KeyboardLayoutMonitor for XkbSwitch {
    fn keyboard_layout(&self) -> Result<String> {
        xkb_switch_show_layout_and_variant().map(|layout_and_variant| layout_and_variant.0)
    }

    fn keyboard_variant(&self) -> Result<String> {
        xkb_switch_show_layout_and_variant()
            .map(|layout_and_variant| layout_and_variant.1.unwrap_or_else(|| "".into()))
    }

    fn must_poll(&self) -> bool {
        true
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct KeyboardLayoutConfig {
    pub format: FormatTemplate,

    driver: KeyboardLayoutDriver,
    #[serde(deserialize_with = "deserialize_duration")]
    interval: Duration,

    sway_kb_identifier: Option<String>,

    // Used to ovrreride long layout names: "German (dead acute)" => "DE"
    mappings: Option<HashMap<String, String>>,
}

impl Default for KeyboardLayoutConfig {
    fn default() -> Self {
        Self {
            format: FormatTemplate::default(),
            driver: KeyboardLayoutDriver::SetXkbMap,
            interval: Duration::from_secs(60),
            sway_kb_identifier: None,
            mappings: None,
        }
    }
}

pub struct KeyboardLayout {
    id: usize,
    output: TextWidget,
    monitor: Box<dyn KeyboardLayoutMonitor>,
    update_interval: Option<Duration>,
    format: FormatTemplate,
    mappings: Option<HashMap<String, String>>,
}

impl ConfigBlock for KeyboardLayout {
    type Config = KeyboardLayoutConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        shared_config: SharedConfig,
        send: Sender<Task>,
    ) -> Result<Self> {
        let monitor: Box<dyn KeyboardLayoutMonitor> = match block_config.driver {
            KeyboardLayoutDriver::SetXkbMap => Box::new(SetXkbMap::new()?),
            KeyboardLayoutDriver::LocaleBus => {
                let monitor = LocaleBus::new()?;
                monitor.monitor(id, send);
                Box::new(monitor)
            }
            KeyboardLayoutDriver::KbddBus => {
                let monitor = KbdDaemonBus::new()?;
                monitor.monitor(id, send);
                Box::new(monitor)
            }
            KeyboardLayoutDriver::XkbSwitch => Box::new(XkbSwitch::new()?),
            KeyboardLayoutDriver::Sway => {
                let monitor = Sway::new(block_config.sway_kb_identifier)?;
                monitor.monitor(id, send);
                Box::new(monitor)
            }
        };
        let update_interval = if monitor.must_poll() {
            Some(block_config.interval)
        } else {
            None
        };
        let output = TextWidget::new(id, 0, shared_config);
        Ok(KeyboardLayout {
            id,
            output,
            monitor,
            update_interval,
            format: block_config.format.with_default("{layout}")?,
            mappings: block_config.mappings,
        })
    }
}

impl Block for KeyboardLayout {
    fn id(&self) -> usize {
        self.id
    }

    fn update(&mut self) -> Result<Option<Update>> {
        let mut layout = self.monitor.keyboard_layout()?;
        let variant = self.monitor.keyboard_variant()?;
        if let Some(ref mappings) = self.mappings {
            if let Some(mapped) = mappings.get(&format!("{} ({})", layout, variant)) {
                layout = mapped.to_string();
            }
        }
        let values = map!(
            "layout" => Value::from_string(layout),
            "variant" => Value::from_string(variant)
        );

        self.output.set_texts(self.format.render(&values)?);
        Ok(self.update_interval.map(|d| d.into()))
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.output]
    }
}

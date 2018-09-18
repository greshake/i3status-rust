use std::env;
use std::fs::File;
use std::io::prelude::*;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use chan::Sender;

use block::{Block, ConfigBlock};
use config::Config;
use errors::*;
use widgets::text::TextWidget;
use widget::I3BarWidget;
use input::I3BarEvent;
use scheduler::Task;

use uuid::Uuid;

extern crate dbus;
extern crate regex;
use self::dbus::{Connection, ConnectionItem, stdintf, arg};
use self::regex::Regex;

pub struct IBus {
    id: String,
    text: TextWidget,
    engine: Arc<Mutex<String>>,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct IBusConfig {
    /// Set to display engine name as icon.
    #[serde(default = "IBusConfig::default_as_icon")]
    pub as_icon: bool,
}

impl IBusConfig {
    fn default_as_icon() -> bool {
        true
    }
}

impl ConfigBlock for IBus {
    type Config = IBusConfig;

    fn new(block_config: Self::Config, config: Config, send: Sender<Task>) -> Result<Self> {
        let id: String = Uuid::new_v4().simple().to_string();
        let id_copy = id.clone();

        let ibus_address = get_ibus_address();
        let c = Connection::open_private(&ibus_address)
            .block_error("ibus", "failed to establish D-Bus connection")?;
        let p = c.with_path("org.freedesktop.IBus", "/org/freedesktop/IBus", 5000);
        use blocks::dbus::stdintf::org_freedesktop_dbus::Properties;
        let info: arg::Variant<Box<arg::RefArg>> = p.get("org.freedesktop.IBus", "GlobalEngine").unwrap();

        // info should contain something containing an array with the contents as such:
        // [name, longname, description, language, license, author, icon, layout, layout_variant, layout_option, rank, hotkeys, symbol, setup, version, textdomain, icon_prop_key]
        // Refer to: https://github.com/ibus/ibus/blob/7cef5bf572596361bc502e8fa917569676a80372/src/ibusenginedesc.c
        // e.g.
        // ["IBusEngineDesc", {}, "xkb:us::eng", "English (US)", "English (US)", "en", "GPL", "Peng Huang <shawn.p.huang@gmail.com>", "ibus-keyboard", "us", 99, "", "", "", "", "", "", "", ""]
        // We will use the 3rd element of the array which corresponds to 'name' (of the current global IBus engine)

        let current = info.0.as_iter().unwrap().nth(2).unwrap().as_str().unwrap_or("??");
        let mut engine_original = Arc::new(Mutex::new(String::from(current)));

        let engine = engine_original.clone();
        thread::spawn(move || {
            let c = Connection::open_private(&ibus_address).unwrap();
            c.add_match(
                "interface='org.freedesktop.IBus',member='GlobalEngineChanged'",
            ).unwrap();
            loop {
                for ci in c.iter(100000) {
                    if let Some(engine_name) = parse_msg(&ci) {
                            let mut engine = engine_original.lock().unwrap();
                            *engine = engine_name.to_string();
                            // tell block to update now
                            send.send(Task {
                                id: id.clone(),
                                update_time: Instant::now(),
                            });
                    };
                }
            }
        });

        Ok(IBus {
            id: id_copy,
            text: TextWidget::new(config.clone()).with_text("IBus"),
            engine
        })
    }
}

impl Block for IBus {
    fn id(&self) -> &str {
        &self.id
    }

    fn update(&mut self) -> Result<Option<Duration>> {
        let mut string = (*self.engine
            .lock()
            .block_error("ibus", "failed to acquire lock")?)
            .clone();
        self.text.set_text(string);
        Ok(None)
    }

    fn view(&self) -> Vec<&I3BarWidget> {
        vec![&self.text]
    }

    fn click(&mut self, _: &I3BarEvent) -> Result<()> {
        Ok(())
    }
}

fn parse_msg(ci: &ConnectionItem) -> Option<&str> {
    let m = if let &ConnectionItem::Signal(ref s) = ci { s } else { return None };
    if &*m.interface().unwrap() != "org.freedesktop.IBus" { return None };
    if &*m.member().unwrap() != "GlobalEngineChanged" { return None };
    let engine = m.get1::<&str>();
    engine
}

fn get_ibus_address() -> String {
    // By default ibus will write the ibus address to the file
    // $XDG_CONFIG_HOME/ibus/bus/aaa-bbb-ccc, along with some other info.
    // with aaa = dbus machine id, usually found at /etc/machine-id
    //      bbb = hostname - seems to be "unix" in most cases [see L99 of reference]
    //      ccc = display number from $DISPLAY
    // Refer to: https://github.com/ibus/ibus/blob/7cef5bf572596361bc502e8fa917569676a80372/src/ibusshare.c
    //
    // Example file contents:
    // # This file is created by ibus-daemon, please do not modify it
    // IBUS_ADDRESS=unix:abstract=/tmp/dbus-8EeieDfT,guid=7542d73dce451c2461a044e24bc131f4
    // IBUS_DAEMON_PID=11140

    let config_dir = env::var("XDG_CONFIG_HOME").expect("env var not set?");

    // Do we need to check /var/lib/dbus/machine-id too ??
    let mut f = File::open("/etc/machine-id").expect("file not found");
    let mut machine_id = String::new();
    f.read_to_string(&mut machine_id).expect("something went wrong reading the file");
    let machine_id = machine_id.trim();

    // TODO: Fix kludge that defaults DISPLAY to '0' on error
    // Had to implement this as when starting sway on first login,
    // $DISPLAY doesn't seem to be set until after swaybar is finished being processed.
    // Possibly because XWayland is only started after swaybar?
    // Even if I add a 10second delay here it will panic with just a simple unwrap.
    let display = env::var("DISPLAY").unwrap_or("0".to_string());
    let re = Regex::new(r"^:(\d{1})$").unwrap();
    let cap = re.captures(&display).unwrap();
    let display = &cap[1].to_string();

    let hostname = String::from("unix");

    let ibus_socket_path = format!("{}/ibus/bus/{}-{}-{}", config_dir, machine_id, hostname, display);

    let mut f = File::open(ibus_socket_path).expect("file not found");
    let mut ibus_address = String::new();
    f.read_to_string(&mut ibus_address).expect("something went wrong reading the file");
    let re = Regex::new(r"IBUS_ADDRESS=(.*),guid").unwrap();
    let cap = re.captures(&ibus_address).unwrap();
    let ibus_address = &cap[1];
    ibus_address.to_string()
}

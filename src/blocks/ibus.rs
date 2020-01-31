use std::env;
use std::fs::{read_dir, File};
use std::io::prelude::*;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use dbus::stdintf::org_freedesktop_dbus::Properties;
use dbus::{arg, Connection, ConnectionItem};
use regex::Regex;
use serde_derive::Deserialize;
use uuid::Uuid;

use crate::blocks::{Block, ConfigBlock};
use crate::config::Config;
use crate::errors::*;
use crate::input::I3BarEvent;
use crate::scheduler::Task;
use crate::util;
use crate::widget::I3BarWidget;
use crate::widgets::text::TextWidget;

pub struct IBus {
    id: String,
    text: TextWidget,
    engine: Arc<Mutex<String>>,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct IBusConfig {}

impl ConfigBlock for IBus {
    type Config = IBusConfig;

    fn new(_block_config: Self::Config, config: Config, send: Sender<Task>) -> Result<Self> {
        let id: String = Uuid::new_v4().to_simple().to_string();
        let id_copy = id.clone();

        let ibus_address = get_ibus_address()?;
        let c = Connection::open_private(&ibus_address).block_error(
            "ibus",
            &format!("Failed to establish D-Bus connection to {}", ibus_address),
        )?;
        let p = c.with_path("org.freedesktop.IBus", "/org/freedesktop/IBus", 5000);
        let info: arg::Variant<Box<dyn arg::RefArg>> = p
            .get("org.freedesktop.IBus", "GlobalEngine")
            .block_error("ibus", "Failed to query IBus")?;

        // `info` should contain something containing an array with the contents as such:
        // [name, longname, description, language, license, author, icon, layout, layout_variant, layout_option, rank, hotkeys, symbol, setup, version, textdomain, icon_prop_key]
        // Refer to: https://github.com/ibus/ibus/blob/7cef5bf572596361bc502e8fa917569676a80372/src/ibusenginedesc.c
        // e.g.                   name           longname        description     language
        // ["IBusEngineDesc", {}, "xkb:us::eng", "English (US)", "English (US)", "en", "GPL", "Peng Huang <shawn.p.huang@gmail.com>", "ibus-keyboard", "us", 99, "", "", "", "", "", "", "", ""]
        //                         ↑ We will use this element (name) as it is what GlobalEngineChanged signal returns.
        let current_engine = info
            .0
            .as_iter()
            .block_error("ibus", "Failed to parse D-Bus message (step 1)")?
            .nth(2)
            .block_error("ibus", "Failed to parse D-Bus message (step 2)")?
            .as_str()
            .unwrap_or("??");
        let engine_original = Arc::new(Mutex::new(String::from(current_engine)));

        let engine = engine_original.clone();
        thread::spawn(move || {
            let c = Connection::open_private(&ibus_address)
                .expect("Failed to establish D-Bus connection in thread");
            c.add_match("interface='org.freedesktop.IBus',member='GlobalEngineChanged'")
                .expect("Failed to add D-Bus message rule - has IBus interface changed?");
            loop {
                for ci in c.iter(100000) {
                    if let Some(engine_name) = parse_msg(&ci) {
                        let mut engine = engine_original.lock().unwrap();
                        *engine = engine_name.to_string();
                        // Tell block to update now.
                        send.send(Task {
                            id: id.clone(),
                            update_time: Instant::now(),
                        })
                        .unwrap();
                    };
                }
            }
        });

        Ok(IBus {
            id: id_copy,
            text: TextWidget::new(config.clone()).with_text("IBus"),
            engine,
        })
    }
}

impl Block for IBus {
    fn id(&self) -> &str {
        &self.id
    }

    // Updates the internal state of the block.
    fn update(&mut self) -> Result<Option<Duration>> {
        let engine = (*self
            .engine
            .lock()
            .block_error("ibus", "failed to acquire lock")?)
        .clone();
        self.text.set_text(engine);
        Ok(None)
    }

    // Returns the view of the block, comprised of widgets.
    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.text]
    }

    // This function is called on every block for every click.
    // TODO: Filter events by using the event.name property,
    // and use to switch between input engines?
    fn click(&mut self, _: &I3BarEvent) -> Result<()> {
        Ok(())
    }
}

fn parse_msg(ci: &ConnectionItem) -> Option<&str> {
    let m = if let &ConnectionItem::Signal(ref s) = ci {
        s
    } else {
        return None;
    };
    if &*m.interface().unwrap() != "org.freedesktop.IBus" {
        return None;
    };
    if &*m.member().unwrap() != "GlobalEngineChanged" {
        return None;
    };
    let engine = m.get1::<&str>();
    engine
}

// Gets the address being used by the currently running ibus daemon.
//
// By default ibus will write the address to `$XDG_CONFIG_HOME/ibus/bus/aaa-bbb-ccc`
// where aaa = dbus machine id, usually found at /etc/machine-id
//       bbb = hostname - seems to be "unix" in most cases [see L99 of reference]
//       ccc = display number from $DISPLAY
// Refer to: https://github.com/ibus/ibus/blob/7cef5bf572596361bc502e8fa917569676a80372/src/ibusshare.c
//
// Example file contents:
// ```
// # This file is created by ibus-daemon, please do not modify it
// IBUS_ADDRESS=unix:abstract=/tmp/dbus-8EeieDfT,guid=7542d73dce451c2461a044e24bc131f4
// IBUS_DAEMON_PID=11140
// ```
fn get_ibus_address() -> Result<String> {
    if let Ok(address) = env::var("IBUS_ADDRESS") {
        return Ok(address);
    }

    let socket_dir = util::xdg_config_home().join("ibus/bus");
    let socket_files: Vec<String> = read_dir(socket_dir.clone())
        .block_error("ibus", &format!("Could not open '{:?}'.", socket_dir))?
        .filter(|entry| entry.is_ok())
        // The path will be valid unicode, so this is safe to unwrap.
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();

    if socket_files.len() == 0 {
        return Err(BlockError(
            "ibus".to_string(),
            "Could not locate an IBus socket file.".to_string(),
        ));
    }

    // Only check $DISPLAY if we need to.

    let socket_path = if socket_files.len() == 1 {
        socket_dir.join(&socket_files[0])
    } else {
        // On sway, $DISPLAY is only set by programs requiring xwayland, such as ibus (GTK2).
        // ibus-daemon can be autostarted by sway (via an entry in config file), however since
        // the bar is executed first, $DISPLAY will not yet be set at the time this code runs.
        // Hence on sway you will need to reload the bar once after login to get the block to work.
        let display_var = env::var("DISPLAY")
            .block_error("ibus", "$DISPLAY not set. Try restarting bar if on sway")?;
        let re = Regex::new(r"^:([0-9]{1})$").unwrap(); // Valid regex is safe to unwrap.
        let cap = re
            .captures(&display_var)
            .block_error("ibus", "Failed to extract display number from $DISPLAY")?;
        let display_number = &cap[1].to_string();

        let candidate = socket_files
            .iter()
            .filter(|fname| fname.ends_with(display_number))
            .take(1)
            .next()
            .block_error(
                "ibus",
                &format!("Could not find an IBus socket file matching $DISPLAY."),
            )?;
        socket_dir.join(candidate)
    };

    let re = Regex::new(r"ADDRESS=(.*),guid").unwrap(); // Valid regex is safe to unwrap.
    let mut address = String::new();
    File::open(&socket_path)
        .block_error("ibus", &format!("Could not open '{:?}'.", socket_path))?
        .read_to_string(&mut address)
        .block_error(
            "ibus",
            &format!("Error reading contents of '{:?}'.", socket_path),
        )?;
    let cap = re.captures(&address).block_error(
        "ibus",
        &format!("Failed to extract address out of '{}'.", address),
    )?;

    Ok(cap[1].to_string())
}

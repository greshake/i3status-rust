use std::collections::BTreeMap;
use std::env;
use std::fs::{read_dir, File};
use std::io::prelude::*;
use std::process::Command;
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Instant;

use crossbeam_channel::Sender;
use dbus::arg::{Array, RefArg};
use dbus::ffidisp::stdintf::org_freedesktop_dbus::Properties;
use dbus::{
    arg,
    ffidisp::{BusType, Connection, ConnectionItem},
    Message,
};
use regex::Regex;
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::SharedConfig;
use crate::errors::*;
use crate::formatting::value::Value;
use crate::formatting::FormatTemplate;
use crate::protocol::i3bar_event::I3BarEvent;
use crate::scheduler::Task;
use crate::util::xdg_config_home;
use crate::widgets::text::TextWidget;
use crate::widgets::I3BarWidget;

pub struct IBus {
    id: usize,
    text: TextWidget,
    engine: Arc<Mutex<String>>,
    mappings: Option<BTreeMap<String, String>>,
    format: FormatTemplate,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct IBusConfig {
    pub mappings: Option<BTreeMap<String, String>>,
    pub format: FormatTemplate,
    /// Text to display on startup when IBus global engine is not yet set.
    pub initial_text: String,
}

impl Default for IBusConfig {
    fn default() -> Self {
        Self {
            mappings: None,
            format: FormatTemplate::default(),
            initial_text: "??".to_string(),
        }
    }
}

impl ConfigBlock for IBus {
    type Config = IBusConfig;

    #[allow(clippy::many_single_char_names)]
    fn new(
        id: usize,
        block_config: Self::Config,
        shared_config: SharedConfig,
        send: Sender<Task>,
    ) -> Result<Self> {
        let init_text = block_config.initial_text;
        let engine_original = Arc::new(Mutex::new(init_text.clone()));

        let c = Connection::get_private(BusType::Session).block_error(
            "ibus",
            "failed to establish D-Bus connection to session bus",
        )?;
        let m = Message::new_method_call(
            "org.freedesktop.DBus",
            "/",
            "org.freedesktop.DBus",
            "ListNames",
        )
        .unwrap();
        let r = c.send_with_reply_and_block(m, 2000).unwrap();
        let arr: Array<&str, _> = r.get1().unwrap();
        // On my system after starting `ibus-daemon` I get `org.freedesktop.IBus`,
        // `org.freedesktop.IBus.Panel.Extension.Gtk3` and `org.freedesktop.portal.IBus`.
        // The last one comes up a while after the other two, and until then any calls to
        // `GlobalEngine` result in a "No global engine" response.
        // Hence the check below to see if there are 3 or more names on the bus with "IBus" in them.
        // TODO: Possibly we only need to check for `org.freedesktop.portal.IBus`? Not sure atm.
        // TODO: Is the Gtk3 one always there? Is it ever a Qt one?
        let running = arr.filter(|entry| entry.contains("IBus")).count() > 2;
        // TODO: revisit this lint
        #[allow(clippy::mutex_atomic)]
        let available = Arc::new((Mutex::new(running), Condvar::new()));
        let available_copy = available.clone();
        let engine_copy = engine_original.clone();
        let send2 = send.clone();
        thread::Builder::new().name("ibus-daemon-monitor".into()).spawn(move || {
            let c = Connection::get_private(BusType::Session).unwrap();
            c.add_match("interface='org.freedesktop.DBus',member='NameOwnerChanged',path='/org/freedesktop/DBus',arg0namespace='org.freedesktop.IBus'")
                .unwrap();
            // Skip the NameAcquired event.
            c.incoming(10_000).next();
            loop {
                for ci in c.iter(100_000) {
                    if let ConnectionItem::Signal(x) = ci {
                    	let (name, old_owner, new_owner): (&str, &str, &str) = x.read3().unwrap();
						if name.contains("IBus") && !old_owner.is_empty() && new_owner.is_empty() {
							let (lock, cvar) = &*available_copy;
							let mut available = lock.lock().unwrap();
							*available = false;
							cvar.notify_one();
                            let mut engine = engine_copy.lock().unwrap();
                            // see comment in the ibus-engine-monitor thread
                            // TODO: way to restart the other thread with the new IBus address
                            eprintln!("ibus block: ibus-daemon was restarted, so the block will no longer update");
                            *engine = "ibus restarted so i3status-rs must be restarted!".to_string();
							send2.send(Task {
								id,
								update_time: Instant::now(),
							}).unwrap();
						} else if name.contains("IBus") && old_owner.is_empty() && !new_owner.is_empty() {
							let (lock, cvar) = &*available_copy;
							let mut available = lock.lock().unwrap();
							*available = true;
							cvar.notify_one();
                            eprintln!("ibus block: ibus-daemon has started!");
							send2.send(Task {
						   		id,
						   		update_time: Instant::now(),
							}).unwrap();
						}
                    }
                }
            }
        }).unwrap();

        let current_engine: String = if running {
            let ibus_address = get_ibus_address()?;
            let c = Connection::open_private(&ibus_address).block_error(
                "ibus",
                &format!("Failed to establish D-Bus connection to {}", ibus_address),
            )?;
            let p = c.with_path("org.freedesktop.IBus", "/org/freedesktop/IBus", 5000);

            // This is a hack.
            // Even when IBus is up and running, this call can error out if an engine has not been set yet.
            // Instead of bringing down the whole bar, we should instead just display a placeholder text.
            let default = Box::new(init_text.clone()) as Box<dyn RefArg>;
            let info: arg::Variant<Box<dyn arg::RefArg>> = p
                .get("org.freedesktop.IBus", "GlobalEngine")
                .unwrap_or(arg::Variant(default));
            let value = &info.0;
            match value.arg_type() {
                arg::ArgType::String => {
                    eprintln!("ibus block: global engine not set");
                    init_text
                }
                arg::ArgType::Struct => {
                    // // `info` should contain something containing an array with the contents as such:
                    // // [name, longname, description, language, license, author, icon, layout, layout_variant, layout_option, rank, hotkeys, symbol, setup, version, textdomain, icon_prop_key]
                    // // Refer to: https://github.com/ibus/ibus/blob/7cef5bf572596361bc502e8fa917569676a80372/src/ibusenginedesc.c
                    // // e.g.                   name           longname        description     language
                    // // ["IBusEngineDesc", {}, "xkb:us::eng", "English (US)", "English (US)", "en", "GPL", "Peng Huang <shawn.p.huang@gmail.com>", "ibus-keyboard", "us", 99, "", "", "", "", "", "", "", ""]
                    // //                         ↑ We will use this element (name) as it is what GlobalEngineChanged signal returns.
                    value
                        .as_iter()
                        .block_error("ibus", "Failed to parse D-Bus message (step 1)")?
                        .nth(2)
                        .block_error("ibus", "Failed to parse D-Bus message (step 2)")?
                        .as_str()
                        .unwrap_or(&init_text)
                        .to_string()
                }
                _ => init_text,
            }
        } else {
            init_text
        };

        let engine_copy2 = engine_original.clone();
        let mut engine = engine_copy2.lock().unwrap();
        *engine = current_engine;

        let engine_copy3 = engine_original.clone();
        thread::Builder::new()
            .name("ibus-engine-monitor".into())
            .spawn(move || {
                // This will pause the thread until we receive word that there
                // is an IBus instance running, so we can avoid panicking if
                // the bar starts before IBus is up.
                // TODO: find a way to restart the loop whenever we detect IBus
                // has restarted. (We need to start a new DBus connection since the
                // address will change.)
                let (lock, cvar) = &*available;
                let mut available = lock.lock().unwrap();
                while !*available {
                    available = cvar.wait(available).unwrap();
                }
                std::mem::drop(available);
                let ibus_address = get_ibus_address().unwrap();
                let c = Connection::open_private(&ibus_address).unwrap_or_else(|_| {
                    panic!("Failed to establish D-Bus connection to {}", ibus_address)
                });
                c.add_match("interface='org.freedesktop.IBus',member='GlobalEngineChanged'")
                    .expect("Failed to add D-Bus message rule - has IBus interface changed?");
                loop {
                    for ci in c.iter(100_000) {
                        if let Some(engine_name) = parse_msg(&ci) {
                            let mut engine = engine_copy3.lock().unwrap();
                            *engine = engine_name.to_string();
                            // Tell block to update now.
                            send.send(Task {
                                id,
                                update_time: Instant::now(),
                            })
                            .unwrap();
                        };
                    }
                }
            })
            .unwrap();

        let text = TextWidget::new(id, 0, shared_config).with_text("IBus");
        Ok(IBus {
            id,
            text,
            engine: engine_original,
            mappings: block_config.mappings,
            format: block_config.format.with_default("{engine}")?,
        })
    }
}

impl Block for IBus {
    fn id(&self) -> usize {
        self.id
    }

    // Updates the internal state of the block.
    fn update(&mut self) -> Result<Option<Update>> {
        let engine = (*self
            .engine
            .lock()
            .block_error("ibus", "failed to acquire lock")?)
        .clone();
        let display_engine = if let Some(m) = &self.mappings {
            match m.get(&engine) {
                Some(mapping) => mapping.to_string(),
                None => engine,
            }
        } else {
            engine
        };

        let values = map!(
            "engine" => Value::from_string(display_engine)
        );

        self.text.set_texts(self.format.render(&values)?);
        Ok(None)
    }

    // Returns the view of the block, comprised of widgets.
    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.text]
    }

    // TODO:
    // switch between input engines?
    fn click(&mut self, _: &I3BarEvent) -> Result<()> {
        Ok(())
    }
}

fn parse_msg(ci: &ConnectionItem) -> Option<&str> {
    let m = if let ConnectionItem::Signal(ref s) = *ci {
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
    m.get1::<&str>()
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
        eprintln!("ibus block: using address from $IBUS_ADDRESS ({})", address);
        return Ok(address);
    }

    // This is the surefire way to get the current IBus address
    if let Ok(address) = Command::new("ibus")
        .args(&["address"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
    {
        eprintln!(
            "ibus block: using address from `ibus address` ({})",
            address
        );
        return Ok(address);
    }

    // If the above fails for some reason, then fallback to guessing the correct socket file
    // TODO: possibly remove all this since if `ibus address` fails then something is wrong
    let socket_dir = xdg_config_home().join("ibus/bus");
    let socket_files: Vec<String> = read_dir(socket_dir.clone())
        .block_error("ibus", &format!("Could not open '{:?}'.", socket_dir))?
        .filter(|entry| entry.is_ok())
        // The path will be valid unicode, so this is safe to unwrap.
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();

    if socket_files.is_empty() {
        return Err(BlockError(
            "ibus".to_string(),
            "Could not locate an IBus socket file.".to_string(),
        ));
    }

    // Only check $DISPLAY if we need to.
    let socket_path = if socket_files.len() == 1 {
        socket_dir.join(&socket_files[0])
    } else {
        let w_display_var = env::var("WAYLAND_DISPLAY");
        let x_display_var = env::var("DISPLAY");

        let display_suffix = if let Ok(x) = w_display_var {
            x
        } else if let Ok(x) = x_display_var {
            let re = Regex::new(r"^:([0-9]{1})$").unwrap(); // Valid regex is safe to unwrap.
            let cap = re
                .captures(&x)
                .block_error("ibus", "Failed to extract display number from $DISPLAY")?;
            cap[1].to_string()
        } else {
            return Err(BlockError(
                "ibus".to_string(),
                "Could not read DISPLAY or WAYLAND_DISPLAY.".to_string(),
            ));
        };

        let candidate = socket_files
            .iter()
            .filter(|fname| fname.ends_with(&display_suffix))
            .take(1)
            .next()
            .block_error(
                "ibus",
                "Could not find an IBus socket file matching $DISPLAY.",
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

    let address = cap[1].to_string();
    eprintln!(
        "ibus block: using address from {} ({})",
        socket_path.into_os_string().into_string().unwrap(),
        address
    );
    Ok(address)
}

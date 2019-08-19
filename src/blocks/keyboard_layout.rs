use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use uuid::Uuid;

use crate::block::{Block, ConfigBlock};
use crate::blocks::dbus;
use crate::blocks::dbus::stdintf::org_freedesktop_dbus::Properties;
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

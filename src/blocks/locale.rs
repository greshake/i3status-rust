use std::thread;
use std::time::{Duration, Instant};

use chan::Sender;
use uuid::Uuid;

use crate::block::{Block, ConfigBlock};
use crate::blocks::dbus;
use crate::blocks::dbus::stdintf::org_freedesktop_dbus::Properties;
use crate::config::Config;
use crate::errors::*;
use crate::scheduler::Task;
use crate::widget::I3BarWidget;
use crate::widgets::text::TextWidget;

pub struct LocaleBus {
    con: dbus::Connection,
}

impl LocaleBus {
    pub fn new() -> Result<Self> {
        let con = dbus::Connection::get_private(dbus::BusType::System)
            .block_error("locale", "Failed to establish D-Bus connection.")?;

        Ok(LocaleBus { con: con })
    }

    pub fn keyboard_layout(&self) -> Result<String> {
        let layout: String = self
            .con
            .with_path("org.freedesktop.locale1", "/org/freedesktop/locale1", 1000)
            .get("org.freedesktop.locale1", "X11Layout")
            .block_error("locale", "Failed to get X11Layout property.")?;

        Ok(layout)
    }

    /// Monitor Locale property changes in a separate thread and send updates
    /// via the `update_request` channel.
    pub fn monitor(&self, id: String, update_request: Sender<Task>) {
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
                    });
                }
            }
        });
    }
}

pub struct Locale {
    id: String,
    output: TextWidget,
    bus: LocaleBus,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct LocaleConfig {}

impl ConfigBlock for Locale {
    type Config = LocaleConfig;

    fn new(_config: Self::Config, config: Config, send: Sender<Task>) -> Result<Self> {
        let id: String = Uuid::new_v4().simple().to_string();
        let bus = LocaleBus::new()?;
        bus.monitor(id.clone(), send);

        Ok(Locale {
            id: id,
            output: TextWidget::new(config),
            bus,
        })
    }
}

impl Block for Locale {
    fn id(&self) -> &str {
        &self.id
    }

    fn update(&mut self) -> Result<Option<Duration>> {
        self.output.set_text(self.bus.keyboard_layout()?);
        Ok(None)
    }

    fn view(&self) -> Vec<&I3BarWidget> {
        vec![&self.output]
    }
}

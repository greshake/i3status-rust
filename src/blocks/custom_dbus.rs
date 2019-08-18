use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;

use uuid::Uuid;

use crate::block::{Block, ConfigBlock};
use crate::blocks::dbus::stdintf::org_freedesktop_dbus::Properties;
use crate::blocks::dbus::stdintf::org_freedesktop_dbus::PropertiesPropertiesChanged;
use crate::blocks::dbus::{BusType, Connection, SignalArgs};
use crate::config::Config;
use crate::errors::*;
use crate::input::I3BarEvent;
use crate::scheduler::Task;

use crate::widget::I3BarWidget;
use crate::widgets::text::TextWidget;

pub struct CustomDBus {
    id: String,
    text: TextWidget,
    status: Arc<Mutex<String>>,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct CustomDBusConfig {
    pub name: String,
}

impl ConfigBlock for CustomDBus {
    type Config = CustomDBusConfig;

    fn new(block_config: Self::Config, config: Config, send: Sender<Task>) -> Result<Self> {
        let id: String = Uuid::new_v4().simple().to_string();
        let id_copy = id.clone();
        let name: String = block_config.name;

        let c = Connection::get_private(BusType::Session).expect("Failed to establish DBus connection.");
        let prop_path = format!("/localhost/statusbar/DBus/{}", name).to_string();
        let p = c.with_path("localhost.statusbar.DBus", &prop_path, 1000);
        let initial_status: String = p.get("localhost.statusbar.DBus", "Status").unwrap_or("??".to_string());
        let status_original = Arc::new(Mutex::new(String::from(initial_status)));
        let status = status_original.clone();

        thread::spawn(move || {
            let c = Connection::get_private(BusType::Session)
                .expect("Failed to establish DBus connection in thread");
            let matched_signal = PropertiesPropertiesChanged::match_str(Some(&"localhost.statusbar.DBus".into()), Some(&format!("/localhost/statusbar/DBus/{}", name).into()));
            c.add_match(&matched_signal)
                .expect("Failed to add DBus message rule");
            loop {
                for msg in c.incoming(1000) {
                    if let Some(signal) = PropertiesPropertiesChanged::from_message(&msg) {
                        let mut status = status_original.lock().unwrap();
                        *status = signal.changed_properties.get("Status").unwrap().0.as_str().unwrap().to_string();
                        // Tell block to update now.
                        send.send(Task {
                            id: id.clone(),
                            update_time: Instant::now(),
                        }).unwrap();
                    };
                }
            }
        });

        Ok(CustomDBus {
            id: id_copy,
            text: TextWidget::new(config.clone()).with_text("CustomDBus"),
            status,
        })
    }
}

impl Block for CustomDBus {
    fn id(&self) -> &str {
        &self.id
    }

    // Updates the internal state of the block.
    fn update(&mut self) -> Result<Option<Duration>> {
        let status = (*self
            .status
            .lock()
            .block_error("custom_dbus", "failed to acquire lock")?)
        .clone();
        self.text.set_text(status);
        Ok(None)
    }

    // Returns the view of the block, comprised of widgets.
    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.text]
    }

    // This function is called on every block for every click.
    fn click(&mut self, _: &I3BarEvent) -> Result<()> {
        Ok(())
    }
}

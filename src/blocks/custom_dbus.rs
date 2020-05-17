use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use dbus::blocking::LocalConnection;
use dbus::tree::Factory;
use serde_derive::Deserialize;
use uuid::Uuid;

use crate::blocks::{Block, ConfigBlock, Update};
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
        let id: String = Uuid::new_v4().to_simple().to_string();
        let id_copy = id.clone();

        let status_original = Arc::new(Mutex::new(String::from("??")));
        let status = status_original.clone();
        thread::Builder::new()
            .name("custom_dbus".into())
            .spawn(move || {
                let mut c = LocalConnection::new_session()
                    .expect("Failed to establish DBus connection in thread");
                c.request_name("i3.status.rs", false, true, false)
                    .expect("Failed to request bus name");

                // TODO: better to rewrite this to use a property?
                let f = Factory::new_fn::<()>();
                let tree = f
                    .tree(())
                    .add(
                        f.object_path(format!("/{}", block_config.name), ())
                            .introspectable()
                            .add(
                                f.interface("i3.status.rs", ()).add_m(
                                    f.method("SetStatus", (), move |m| {
                                        // This is the callback that will be called when another peer on the bus calls our method.
                                        // the callback receives "MethodInfo" struct and can return either an error, or a list of
                                        // messages to send back.

                                        let new_status: &str = m.msg.read1()?;
                                        let mut status = status_original.lock().unwrap();
                                        *status = new_status.to_string();

                                        // Tell block to update now.
                                        send.send(Task {
                                            id: id.clone(),
                                            update_time: Instant::now(),
                                        })
                                        .unwrap();

                                        Ok(vec![m.msg.method_return()])
                                    })
                                    .inarg::<&str, _>("name"), // We also add the signal to the interface. This is mainly for introspection.
                                ),
                            ),
                    )
                    .add(f.object_path("/", ()).introspectable());

                // We add the tree to the connection so that incoming method calls will be handled.
                tree.start_receive(&c);

                // Serve clients forever.
                loop {
                    c.process(Duration::from_millis(1000)).unwrap();
                }
            })
            .unwrap();

        Ok(CustomDBus {
            id: id_copy,
            text: TextWidget::new(config).with_text("CustomDBus"),
            status,
        })
    }
}

impl Block for CustomDBus {
    fn id(&self) -> &str {
        &self.id
    }

    // Updates the internal state of the block.
    fn update(&mut self) -> Result<Option<Update>> {
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
    // TODO: Filter events by using the event.name property,
    // and use to switch between input engines?
    fn click(&mut self, _: &I3BarEvent) -> Result<()> {
        Ok(())
    }
}

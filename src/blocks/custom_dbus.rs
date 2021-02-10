use std::collections::BTreeMap;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use dbus::blocking::LocalConnection;
use dbus::strings::Signature;
use dbus::tree::Factory;
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::Config;
use crate::errors::*;
use crate::input::I3BarEvent;
use crate::scheduler::Task;
use crate::widget::{I3BarWidget, State};
use crate::widgets::text::TextWidget;

#[derive(Clone)]
struct CustomDBusStatus {
    content: String,
    icon: String,
    state: State,
}

pub struct CustomDBus {
    id: usize,
    text: TextWidget,
    status: Arc<Mutex<CustomDBusStatus>>,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct CustomDBusConfig {
    pub name: String,

    #[serde(default = "CustomDBusConfig::default_color_overrides")]
    pub color_overrides: Option<BTreeMap<String, String>>,
}

impl CustomDBusConfig {
    fn default_color_overrides() -> Option<BTreeMap<String, String>> {
        None
    }
}

impl ConfigBlock for CustomDBus {
    type Config = CustomDBusConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        config: Config,
        send: Sender<Task>,
    ) -> Result<Self> {
        let status_original = Arc::new(Mutex::new(CustomDBusStatus {
            content: String::from("??"),
            icon: String::from(""),
            state: State::Idle,
        }));
        let status = status_original.clone();
        let name = block_config.name;
        thread::Builder::new()
            .name("custom_dbus".into())
            .spawn(move || {
                let c = LocalConnection::new_session()
                    .expect("Failed to establish DBus connection in thread");
                c.request_name("i3.status.rs", false, true, false)
                    .expect("Failed to request bus name");

                // TODO: better to rewrite this to use a property?
                let f = Factory::new_fn::<()>();
                let tree = f
                    .tree(())
                    .add(
                        f.object_path(format!("/{}", name), ())
                            .introspectable()
                            .add(
                                f.interface("i3.status.rs", ()).add_m(
                                    f.method("SetStatus", (), move |m| {
                                        // This is the callback that will be called when another peer on the bus calls our method.
                                        // the callback receives "MethodInfo" struct and can return either an error, or a list of
                                        // messages to send back.

                                        let args = m.msg.get3::<&str, &str, &str>();
                                        let mut status = status_original.lock().unwrap();

                                        if let Some(new_content) = args.0 {
                                            status.content = String::from(new_content);
                                        }

                                        if let Some(new_icon) = args.1 {
                                            status.icon = String::from(new_icon);
                                        }

                                        if let Some(new_state) = args.2 {
                                            status.state =
                                                State::from_str(new_state).unwrap_or(status.state);
                                        }

                                        // Tell block to update now.
                                        send.send(Task {
                                            id,
                                            update_time: Instant::now(),
                                        })
                                        .unwrap();

                                        Ok(vec![m.msg.method_return()])
                                    })
                                    // We also add the signal to the interface. This is mainly for introspection.
                                    .in_args(vec![
                                        ("name", Signature::make::<&str>()),
                                        ("icon", Signature::make::<&str>()),
                                        ("state", Signature::make::<&str>()),
                                    ]),
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

        let text = TextWidget::new(config, id).with_text("CustomDBus");
        Ok(CustomDBus { id, text, status })
    }
}

impl Block for CustomDBus {
    fn id(&self) -> usize {
        self.id
    }

    // Updates the internal state of the block.
    fn update(&mut self) -> Result<Option<Update>> {
        let status = (*self
            .status
            .lock()
            .block_error("custom_dbus", "failed to acquire lock")?)
        .clone();
        self.text.set_text(status.content);
        self.text.set_icon(&status.icon);
        self.text.set_state(status.state);
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

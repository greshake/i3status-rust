use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use dbus::blocking::LocalConnection;
use dbus::strings::Signature;
use dbus_tree::Factory;
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::SharedConfig;
use crate::de::deserialize_opt_duration;
use crate::errors::*;
use crate::scheduler::Task;
use crate::widgets::text::TextWidget;
use crate::widgets::{I3BarWidget, State};

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
    timeout: Option<Duration>,
    clear_pending: Option<Instant>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct CustomDBusConfig {
    pub name: String,

    /// Text to display on startup until the first update is received on the bus.
    pub initial_text: String,

    /// Timeout for clearing the block output after an update (in seconds)
    #[serde(default, deserialize_with = "deserialize_opt_duration")]
    pub timeout: Option<Duration>,
}

impl Default for CustomDBusConfig {
    fn default() -> Self {
        Self {
            name: Default::default(),
            initial_text: "??".to_string(),
            timeout: None,
        }
    }
}

impl ConfigBlock for CustomDBus {
    type Config = CustomDBusConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        shared_config: SharedConfig,
        send: Sender<Task>,
    ) -> Result<Self> {
        let status_original = Arc::new(Mutex::new(CustomDBusStatus {
            content: block_config.initial_text,
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

        let text = TextWidget::new(id, 0, shared_config).with_text("CustomDBus");
        Ok(CustomDBus {
            id,
            text,
            status,
            timeout: block_config.timeout,
            clear_pending: None,
        })
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

        let now = Instant::now();
        if let Some(time) = self.clear_pending {
            if time < now {
                self.clear_pending = None;
                self.text.set_text(String::from(""));
                return Ok(None);
            }
        }

        self.text.set_text(status.content);
        if status.icon.is_empty() {
            self.text.unset_icon();
        } else {
            self.text.set_icon(&status.icon)?;
        }
        self.text.set_state(status.state);

        if let Some(delay) = self.timeout {
            self.clear_pending = Some(now + delay);
            Ok(Some(delay.into()))
        } else {
            Ok(None)
        }
    }

    // Returns the view of the block, comprised of widgets.
    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.text]
    }
}

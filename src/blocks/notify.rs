use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

use crossbeam_channel::Sender;
use dbus::ffidisp::stdintf::org_freedesktop_dbus::{Properties, PropertiesPropertiesChanged};
use dbus::ffidisp::{BusType, Connection};
use dbus::message::SignalArgs;
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::SharedConfig;
use crate::errors::*;
use crate::input::{I3BarEvent, MouseButton};
use crate::scheduler::Task;
use crate::util::{pseudo_uuid, FormatTemplate};
use crate::widgets::text::TextWidget;
use crate::widgets::I3BarWidget;

// TODO
// Add driver option so can choose between dunst, mako, etc.

pub struct Notify {
    id: usize,
    paused: Arc<Mutex<i64>>,
    format: FormatTemplate,
    output: TextWidget,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct NotifyConfig {
    /// Format string for displaying phone information.
    #[serde(default = "NotifyConfig::default_format")]
    pub format: String,
}

impl NotifyConfig {
    fn default_format() -> String {
        // display just the bell icon
        "".into()
    }
}

impl ConfigBlock for Notify {
    type Config = NotifyConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        shared_config: SharedConfig,
        send: Sender<Task>,
    ) -> Result<Self> {
        let notify_id = pseudo_uuid();

        let c = Connection::get_private(BusType::Session).block_error(
            "notify",
            &"Failed to establish D-Bus connection".to_string(),
        )?;

        let p = c.with_path(
            "org.freedesktop.Notifications",
            "/org/freedesktop/Notifications",
            5000,
        );
        let initial_state: bool = p.get("org.dunstproject.cmd0", "paused").block_error(
            "notify",
            &"Failed to get dunst state. Is it running?".to_string(),
        )?;

        let icon = if initial_state { "bell-slash" } else { "bell" };

        // TODO: revisit this lint
        #[allow(clippy::mutex_atomic)]
        let state = Arc::new(Mutex::new(initial_state as i64));
        let state_copy = state.clone();

        thread::Builder::new()
            .name("notify".into())
            .spawn(move || {
                let c = Connection::get_private(BusType::Session)
                    .expect("Failed to establish D-Bus connection in thread");

                let matched_signal = PropertiesPropertiesChanged::match_str(
                    Some(&"org.freedesktop.Notifications".into()),
                    None,
                );
                c.add_match(&matched_signal).unwrap();
                loop {
                    for msg in c.incoming(1000) {
                        if let Some(signal) = PropertiesPropertiesChanged::from_message(&msg) {
                            let value = signal.changed_properties.get("paused").unwrap();
                            let status = &value.0.as_i64().unwrap();
                            let mut paused = state_copy.lock().unwrap();
                            *paused = *status;

                            // Tell block to update now.
                            send.send(Task {
                                id,
                                update_time: Instant::now(),
                            })
                            .unwrap();
                        }
                    }
                }
            })
            .unwrap();

        Ok(Notify {
            id,
            paused: state,
            format: FormatTemplate::from_string(&block_config.format)?,
            output: TextWidget::new(notify_id, 0, shared_config).with_icon(icon),
        })
    }
}

impl Block for Notify {
    fn id(&self) -> usize {
        self.id
    }

    fn update(&mut self) -> Result<Option<Update>> {
        let paused = *self
            .paused
            .lock()
            .block_error("notify", "failed to acquire lock for `state`")?;

        let values = map!(
            "{state}" => paused.to_string()
        );

        self.output
            .set_text(self.format.render_static_str(&values)?);

        let icon = if paused == 1 { "bell-slash" } else { "bell" };
        self.output.set_icon(icon);

        Ok(None)
    }

    // Returns the view of the block, comprised of widgets.
    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.output]
    }

    fn click(&mut self, e: &I3BarEvent) -> Result<()> {
        if let MouseButton::Left = e.button {
            let c = Connection::get_private(BusType::Session).block_error(
                "notify",
                &"Failed to establish D-Bus connection".to_string(),
            )?;

            let p = c.with_path(
                "org.freedesktop.Notifications",
                "/org/freedesktop/Notifications",
                5000,
            );

            let paused = *self
                .paused
                .lock()
                .block_error("notify", "failed to acquire lock")?;

            if paused == 1 {
                p.set("org.dunstproject.cmd0", "paused", false)
                    .block_error("notify", &"Failed to query D-Bus".to_string())?;
            } else {
                p.set("org.dunstproject.cmd0", "paused", true)
                    .block_error("notify", &"Failed to query D-Bus".to_string())?;
            }

            // block will auto-update due to monitoring the bus
        }
        Ok(())
    }
}

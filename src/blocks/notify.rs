use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

use crossbeam_channel::Sender;
use dbus::ffidisp::stdintf::org_freedesktop_dbus::{Properties, PropertiesPropertiesChanged};
use dbus::ffidisp::{BusType, Connection};
use dbus::message::SignalArgs;
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::Config;
use crate::errors::*;
use crate::input::{I3BarEvent, MouseButton};
use crate::scheduler::Task;
use crate::util::{pseudo_uuid, FormatTemplate};
use crate::widget::I3BarWidget;
use crate::widgets::button::ButtonWidget;

// TODO
// Add driver option so can choose between dunst, mako, etc.

pub struct Notify {
    id: usize,
    notify_id: usize,
    paused: Arc<Mutex<i64>>,
    format: FormatTemplate,
    output: ButtonWidget,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct NotifyConfig {
    /// Format string for displaying phone information.
    #[serde(default = "NotifyConfig::default_format")]
    pub format: String,

    #[serde(default = "NotifyConfig::default_color_overrides")]
    pub color_overrides: Option<BTreeMap<String, String>>,
}

impl NotifyConfig {
    fn default_format() -> String {
        // display just the bell icon
        "".into()
    }

    fn default_color_overrides() -> Option<BTreeMap<String, String>> {
        None
    }
}

impl ConfigBlock for Notify {
    type Config = NotifyConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        config: Config,
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
            notify_id,
            paused: state,
            format: FormatTemplate::from_string(&block_config.format)?,
            output: ButtonWidget::new(config, notify_id).with_icon(icon),
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
        if e.matches_id(self.notify_id) {
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
        }
        Ok(())
    }
}

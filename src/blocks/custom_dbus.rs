//! A block controled by the DBus
//!
//! This block creates a new DBus object in `rs.i3status` service. This object implements
//! `rs.i3status.custom` interface which allows you to set block's icon, text and state.
//!
//! Output of `busctl --user introspect rs.i3status /<path> rs.i3status.custom`:
//! ```text
//! NAME                                TYPE      SIGNATURE RESULT/VALUE FLAGS
//! rs.i3status.custom                  interface -         -            -
//! .SetIcon                            method    s         s            -
//! .SetState                           method    s         s            -
//! .SetText                            method    ss        s            -
//! ```
//!
//! # Example
//!
//! Config:
//! ```toml
//! [[block]]
//! block = "custom_dbus"
//! path = "/my_path"
//! ```
//!
//! Useage:
//! ```sh
//! # set full text to 'hello' and short text to 'hi'
//! busctl --user call rs.i3status /my_path rs.i3status.custom SetText ss hello hi
//! # set icon to 'music'
//! busctl --user call rs.i3status /my_path rs.i3status.custom SetIcon s music
//! # set state to 'good'
//! busctl --user call rs.i3status /my_path rs.i3status.custom SetState s good
//! ```
//!
//! # TODO
//! - Send a signal on click?

use super::prelude::*;
use zbus::dbus_interface;

// Share DBus connection between multiple block instances
static DBUS_CONNECTION: async_once_cell::OnceCell<Result<zbus::Connection>> =
    async_once_cell::OnceCell::new();

const DBUS_NAME: &str = "rs.i3status";

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct CustomDBusConfig {
    path: String,
}

struct Block {
    widget: Widget,
    api: CommonApi,
}

#[dbus_interface(name = "rs.i3status.custom")]
impl Block {
    async fn set_icon(&mut self, icon: &str) -> String {
        if let Err(e) = self.widget.set_icon(icon) {
            return e.to_string();
        }
        if let Err(e) = self.api.set_widget(&self.widget).await {
            return e.to_string();
        }
        "OK".into()
    }

    async fn set_text(&mut self, full: String, short: String) -> String {
        self.widget.set_texts(full, short);
        if let Err(e) = self.api.set_widget(&self.widget).await {
            return e.to_string();
        }
        "OK".into()
    }

    async fn set_state(&mut self, state: &str) -> String {
        self.widget.state = match state {
            "idle" => State::Idle,
            "info" => State::Info,
            "good" => State::Good,
            "warning" => State::Warning,
            "critical" => State::Critical,
            _ => return format!("'{state}' is not a valid state"),
        };
        if let Err(e) = self.api.set_widget(&self.widget).await {
            return e.to_string();
        }
        "OK".into()
    }
}

pub async fn run(config: toml::Value, mut api: CommonApi) -> Result<()> {
    // This block doesn't listen for any events. Closing the channel is necessary, because channels
    // are bounded by the number of pending messages, and if we don't close it now, main thread
    // will get blocked while trying to send a new message.
    api.event_receiver.close();

    let path = CustomDBusConfig::deserialize(config).config_error()?.path;
    let dbus_conn = DBUS_CONNECTION
        .get_or_init(dbus_conn())
        .await
        .as_ref()
        .map_err(|e| Error::new(e.to_string()))?;
    dbus_conn
        .object_server()
        .at(
            path,
            Block {
                widget: api.new_widget(),
                api,
            },
        )
        .await
        .error("Failed to setup DBus server")?;
    Ok(())
}

async fn dbus_conn() -> Result<zbus::Connection> {
    let conn = new_dbus_connection().await?;
    conn.request_name(DBUS_NAME)
        .await
        .error("Failed to reuqest DBus name")?;
    Ok(conn)
}

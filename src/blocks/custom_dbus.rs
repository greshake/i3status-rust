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

use zbus::dbus_interface;

use super::prelude::*;

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct CustomDBusConfig {
    path: StdString,
}

struct Block {
    api: CommonApi,
}

#[dbus_interface(name = "rs.i3status.custom")]
impl Block {
    async fn set_icon(&mut self, icon: &str) -> StdString {
        if let Err(e) = self.api.set_icon(icon) {
            return e.to_string();
        }
        if let Err(e) = self.api.flush().await {
            return e.to_string();
        }
        "OK".into()
    }

    async fn set_text(&mut self, full: StdString, short: StdString) -> StdString {
        self.api.set_texts(full.into(), short.into());
        if let Err(e) = self.api.flush().await {
            return e.to_string();
        }
        "OK".into()
    }

    async fn set_state(&mut self, state: &str) -> StdString {
        match state {
            "idle" => self.api.set_state(State::Idle),
            "info" => self.api.set_state(State::Info),
            "good" => self.api.set_state(State::Good),
            "warning" => self.api.set_state(State::Warning),
            "critical" => self.api.set_state(State::Critical),
            _ => return format!("'{state}' is not a valid state"),
        }
        if let Err(e) = self.api.flush().await {
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
    let dbus_conn = api.get_dbus_connection().await?;
    dbus_conn
        .object_server()
        .at(path, Block { api })
        .await
        .error("Failed to setup DBus server")?;
    Ok(())
}

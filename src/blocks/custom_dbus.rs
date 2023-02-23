//! A block controlled by the DBus
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
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `format` | A string to customise the output of this block. | <code>"{ $icon&vert;}{ $text.str(pango:true)&vert;} "</code>
//!
//! Placeholder  | Value                                  | Type   | Unit
//! -------------|-------------------------------------------------------------------|--------|---------------
//! `icon`       | Value of icon set via `SetIcon` if the value is non-empty string. | Icon   | -
//! `text`       | Value of the first string from SetText                            | Text   | -
//! `short_text` | Value of the second string from SetText                           | Text   | -
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
//! Usage:
//! ```sh
//! # set full text to 'hello' and short text to 'hi'
//! busctl --user call rs.i3status /my_path rs.i3status.custom SetText ss hello hi
//! # set icon to 'music'
//! busctl --user call rs.i3status /my_path rs.i3status.custom SetIcon s music
//! # set state to 'good'
//! busctl --user call rs.i3status /my_path rs.i3status.custom SetState s good
//! ```
//!
//! Because it's impossible to publish objects to the same name from different
//! processes, having multiple dbus blocks in different bars won't work. As a workaround,
//! you can set the env var `I3RS_DBUS_NAME` to set the interface a bar works on to
//! differentiate between different processes. For example, setting this to 'top', will allow you
//! to use `rs.i3status.top`.
//!
//! # TODO
//! - Send a signal on click?

use super::prelude::*;
use std::env;
use zbus::{dbus_interface, fdo};

// Share DBus connection between multiple block instances
static DBUS_CONNECTION: async_once_cell::OnceCell<Result<zbus::Connection>> =
    async_once_cell::OnceCell::new();

const DBUS_NAME: &str = "rs.i3status";

#[derive(Deserialize, Debug)]
pub struct Config {
    #[serde(default)]
    format: FormatConfig,
    path: String,
}

struct Block {
    widget: Widget,
    api: CommonApi,
    icon: Option<String>,
    text: Option<String>,
    short_text: Option<String>,
}

fn block_values(block: &Block, api: &CommonApi) -> Result<HashMap<Cow<'static, str>, Value>> {
    Ok(map! {
        [if let Some(icon) = &block.icon] "icon" => Value::icon(api.get_icon(icon)?),
        [if let Some(text) = &block.text] "text" => Value::text(text.to_string()),
        [if let Some(short_text) = &block.short_text] "short_text" => Value::text(short_text.to_string()),
    })
}

#[dbus_interface(name = "rs.i3status.custom")]
impl Block {
    async fn set_icon(&mut self, icon: &str) -> fdo::Result<()> {
        self.icon = if icon.is_empty() {
            None
        } else {
            Some(icon.to_string())
        };
        self.widget.set_values(block_values(self, &self.api)?);
        self.api.set_widget(&self.widget).await?;
        Ok(())
    }

    async fn set_text(&mut self, full: String, short: String) -> fdo::Result<()> {
        self.text = Some(full);
        self.short_text = Some(short);
        self.widget.set_values(block_values(self, &self.api)?);
        self.api.set_widget(&self.widget).await?;
        Ok(())
    }

    async fn set_state(&mut self, state: &str) -> fdo::Result<()> {
        self.widget.state = match state {
            "idle" => State::Idle,
            "info" => State::Info,
            "good" => State::Good,
            "warning" => State::Warning,
            "critical" => State::Critical,
            _ => return Err(Error::new(format!("'{state}' is not a valid state")).into()),
        };
        self.api.set_widget(&self.widget).await?;
        Ok(())
    }
}

pub async fn run(config: Config, mut api: CommonApi) -> Result<()> {
    // This block doesn't listen for any events. Closing the channel is necessary, because channels
    // are bounded by the number of pending messages, and if we don't close it now, main thread
    // will get blocked while trying to send a new message.
    api.event_receiver.close();

    let widget = Widget::new().with_format(config.format.with_defaults(
        "{ $icon|}{ $text.str(pango:true)|} ",
        "{ $icon|} $short_text.str(pango:true) |",
    )?);

    let dbus_conn = DBUS_CONNECTION
        .get_or_init(dbus_conn())
        .await
        .as_ref()
        .map_err(Clone::clone)?;
    dbus_conn
        .object_server()
        .at(
            config.path,
            Block {
                widget,
                api,
                icon: None,
                text: None,
                short_text: None,
            },
        )
        .await
        .error("Failed to setup DBus server")?;
    Ok(())
}

async fn dbus_conn() -> Result<zbus::Connection> {
    let dbus_interface_name = match env::var("I3RS_DBUS_NAME") {
        Ok(v) => format!("{DBUS_NAME}.{v}"),
        Err(_) => DBUS_NAME.to_string(),
    };

    let conn = new_dbus_connection().await?;
    conn.request_name(dbus_interface_name)
        .await
        .error("Failed to request DBus name")?;
    Ok(conn)
}

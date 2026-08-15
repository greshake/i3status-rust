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
//! `format` | A string to customise the output of this block. | <code>\"{ $icon\|}{ $text.pango-str()\|} \"</code>
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
use zbus::fdo;

// Share DBus connection between multiple block instances
static DBUS_CONNECTION: tokio::sync::OnceCell<Result<zbus::Connection>> =
    tokio::sync::OnceCell::const_new();

const DBUS_NAME: &str = "rs.i3status";

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub format: FormatConfig,
    pub path: String,
}

struct Block {
    widget: Widget,
    api: CommonApi,
    icon: Option<String>,
    text: Option<String>,
    short_text: Option<String>,
}

fn block_values(block: &Block) -> HashMap<Cow<'static, str>, Value> {
    map! {
        [if let Some(icon) = &block.icon] "icon" => Value::icon(icon.to_string()),
        [if let Some(text) = &block.text] "text" => Value::text(text.to_string()),
        [if let Some(short_text) = &block.short_text] "short_text" => Value::text(short_text.to_string()),
    }
}

#[zbus::interface(name = "rs.i3status.custom")]
impl Block {
    async fn set_icon(&mut self, icon: &str) -> fdo::Result<()> {
        self.icon = if icon.is_empty() {
            None
        } else {
            Some(icon.to_string())
        };
        self.widget.set_values(block_values(self));
        self.api.set_widget(self.widget.clone())?;
        Ok(())
    }

    async fn set_text(&mut self, full: String, short: String) -> fdo::Result<()> {
        self.text = Some(full);
        self.short_text = Some(short);
        self.widget.set_values(block_values(self));
        self.api.set_widget(self.widget.clone())?;
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
        self.api.set_widget(self.widget.clone())?;
        Ok(())
    }
}

pub(crate) fn prepare(config: &Config) -> Result<Arc<BlockPlan>> {
    // The icon name arrives over D-Bus at runtime, so any name is permitted;
    // it resolves through the normal icon set and override rules.
    BlockPlan::new(vec![
        OutputPlan::new(
            "main",
            config.format.with_defaults(
                "{ $icon|}{ $text.pango-str()|} ",
                "{ $icon|} $short_text.pango-str() | ",
            )?,
        )
        .icon("icon", IconChoices::OpenResolvable),
    ])
}

pub(crate) async fn run(config: &Config, api: &CommonApi, plan: &Arc<BlockPlan>) -> Result<()> {
    let output = plan.output("main")?;
    let widget = output.new_widget();

    let dbus_conn = DBUS_CONNECTION
        .get_or_init(dbus_conn)
        .await
        .as_ref()
        .map_err(Clone::clone)?;
    dbus_conn
        .object_server()
        .at(
            config.path.clone(),
            Block {
                widget,
                api: api.clone(),
                icon: None,
                text: None,
                short_text: None,
            },
        )
        .await
        .error("Failed to setup DBus server")?;
    Ok(())
}

/// Doctor-only override, set in-process by the doctor worker before the
/// block runs. It never touches the environment, so if_command and custom
/// commands observe exactly what the user's shell exported.
static DOCTOR_NAME_OVERRIDE: std::sync::OnceLock<String> = std::sync::OnceLock::new();

pub(crate) fn set_doctor_dbus_suffix(suffix: String) {
    let _ = DOCTOR_NAME_OVERRIDE.set(suffix);
}

/// The well-known name to request. The doctor override takes precedence
/// over the documented public `I3RS_DBUS_NAME`; it is not part of the
/// public interface.
fn dbus_name(doctor_override: Option<&str>, public: Option<&str>) -> String {
    match doctor_override.or(public) {
        Some(v) => format!("{DBUS_NAME}.{v}"),
        None => DBUS_NAME.to_string(),
    }
}

async fn dbus_conn() -> Result<zbus::Connection> {
    let public = env::var("I3RS_DBUS_NAME").ok();
    let dbus_interface_name = dbus_name(
        DOCTOR_NAME_OVERRIDE.get().map(String::as_str),
        public.as_deref(),
    );

    let conn = new_dbus_connection().await?;
    conn.request_name(dbus_interface_name)
        .await
        .error("Failed to request DBus name")?;
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(toml: &str) -> Config {
        toml::from_str(toml).unwrap()
    }

    #[test]
    fn plan_declares_an_open_icon_because_the_name_arrives_over_dbus() {
        let plan = prepare(&config(r#"path = "/my_path""#)).unwrap();
        let ids: Vec<_> = plan.outputs().map(|o| o.id()).collect();
        assert_eq!(ids, ["main"]);

        let output = plan.output("main").unwrap();
        let choices = output.output().choices_for("icon").unwrap();
        assert!(matches!(choices, IconChoices::OpenResolvable));
        assert!(choices.permits("any_name_set_over_dbus"));
        // Whatever name arrives over D-Bus passes the publish-time check,
        // which is where the contract is enforced.
        let mut widget = output.new_widget();
        widget.set_values(map!("icon" => Value::icon("whatever_was_sent")));
        widget.check_contract().unwrap();
    }

    #[test]
    fn custom_format_is_respected() {
        // The default short format also carries `$icon`, so both halves have
        // to be overridden for the icon to be gone from the output.
        let plan = prepare(&config(
            r#"
            path = "/my_path"
            format = { full = " $text.pango-str() ", short = " $short_text.pango-str() " }
            "#,
        ))
        .unwrap();
        let output = plan.output("main").unwrap();
        assert!(output.format().contains_key("text"));
        assert!(output.format().contains_key("short_text"));
        assert!(!output.format().contains_key("icon"));
    }

    #[test]
    fn doctor_override_wins_without_touching_the_public_name() {
        assert_eq!(dbus_name(None, None), "rs.i3status");
        assert_eq!(dbus_name(None, Some("top")), "rs.i3status.top");
        // Doctor workers set only the internal override; the public
        // variable keeps whatever value the user's environment has.
        assert_eq!(dbus_name(Some("doctor3"), None), "rs.i3status.doctor3");
        assert_eq!(
            dbus_name(Some("doctor3"), Some("top")),
            "rs.i3status.doctor3"
        );
    }
}

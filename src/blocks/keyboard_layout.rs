//! Keyboard layout indicator
//!
//! Four drivers are available:
//! - `setxkbmap` which polls setxkbmap to get the current layout
//! - `localebus` which can read asynchronous updates from the systemd `org.freedesktop.locale1` D-Bus path
//! - `kbddbus` which uses [kbdd](https://github.com/qnikst/kbdd) to monitor per-window layout changes via DBus
//! - `sway` which can read asynchronous updates from the sway IPC
//!
//! Which of these methods is appropriate will depend on your system setup.
//!
//! # Configuration
//!
//! Key | Values | Required | Default
//! ----|--------|----------|--------
//! `driver` | One of `"setxkbmap"`, `"localebus"`, `"kbddbus"` or `"sway"`, depending on your system. | No | `"setxkbmap"`
//! `interval` | Update interval, in seconds. Only used by the `"setxkbmap"` driver. | No | `60`
//! `format` | A string to customise the output of this block. See below for available placeholders. | No | `"$layout"`
//! `sway_kb_identifier` | Identifier of the device you want to monitor, as found in the output of `swaymsg -t get_inputs`. | No | Defaults to first input found
//! `mappings` | Map `layout (variant)` to custom short name. | No | None
//!
//!  Key     | Value | Type
//! ---------|-------|-----
//! `layout` | Keyboard layout name | String
//! `variant`| Keyboard variant. Only `localebus` and `sway` are supported so far. | String
//!
//! # Examples
//!
//! Check `setxkbmap` every 15 seconds:
//!
//! ```toml
//! [[block]]
//! block = "keyboard_layout"
//! driver = "setxkbmap"
//! interval = 15
//! ```
//!
//! Listen to D-Bus for changes:
//!
//! ```toml
//! [[block]]
//! block = "keyboard_layout"
//! driver = "localebus"
//! ```
//!
//! Listen to kbdd for changes:
//!
//! ```toml
//! [[block]]
//! block = "keyboard_layout"
//! driver = "kbddbus"
//! ```
//!
//! Listen to sway for changes:
//!
//! ```toml
//! [[block]]
//! block = "keyboard_layout"
//! driver = "sway"
//! sway_kb_identifier = "1133:49706:Gaming_Keyboard_G110"
//! ```
//!
//! Listen to sway for changes and override mappings:
//! ```toml
//! [[block]]
//! block = "keyboard_layout"
//! driver = "sway"
//! format = "$layout"
//! [block.mappings]
//! "English (Workman)" = "EN"
//! "Russian (N/A)" = "RU"
//! ```

use std::collections::HashMap;

use swayipc_async::{Connection, Event, EventType};

use tokio::process::Command;

use zbus::dbus_proxy;

use super::prelude::*;

#[derive(Deserialize, Debug, Derivative)]
#[serde(default, deny_unknown_fields)]
#[derivative(Default)]
struct KeyboardLayoutConfig {
    format: FormatConfig,
    driver: KeyboardLayoutDriver,
    #[derivative(Default(value = "60.into()"))]
    interval: Seconds,
    sway_kb_identifier: Option<String>,
    mappings: Option<HashMap<StdString, String>>,
}

#[derive(Deserialize, Debug, Derivative, Clone, Copy)]
#[serde(rename_all = "lowercase")]
#[derivative(Default)]
enum KeyboardLayoutDriver {
    #[derivative(Default)]
    SetXkbMap,
    LocaleBus,
    KbddBus,
    Sway,
}

pub async fn run(config: toml::Value, mut api: CommonApi) -> Result<()> {
    let config = KeyboardLayoutConfig::deserialize(config).config_error()?;
    api.set_format(config.format.with_default("$layout")?);

    let send = move |(mut layout, variant): (String, Option<String>), api: &mut CommonApi| {
        let variant = variant.unwrap_or_else(|| "N/A".into());
        if let Some(mappings) = &config.mappings {
            if let Some(mapped) = mappings.get(&format!("{} ({})", layout, variant)) {
                layout = mapped.clone();
            }
        }

        api.set_values(map! {
            "layout" => Value::text(layout),
            "variant" => Value::text(variant),
        });
    };

    match config.driver {
        // Just run "setxkbmap" commnad every N seconds and parse it's output
        KeyboardLayoutDriver::SetXkbMap => {
            loop {
                let output = Command::new("setxkbmap")
                    .arg("-query")
                    .output()
                    .await
                    .error("Failed to execute setxkbmap")?;
                let output = StdString::from_utf8(output.stdout)
                    .error("setxkbmap produced a non-UTF8 output")?;
                let layout = output
                    .lines()
                    // Find the "layout:    xxxx" entry.
                    .find(|line| line.starts_with("layout"))
                    .error("Could not find the layout entry from setxkbmap")?
                    .split_ascii_whitespace()
                    .last()
                    .error("Could not read the layout entry from setxkbmap.")?;

                send((layout.into(), None), &mut api);
                api.flush().await?;

                sleep(config.interval.0).await
            }
        }
        KeyboardLayoutDriver::LocaleBus => {
            let conn = api.get_system_dbus_connection().await?;
            let proxy = LocaleBusProxy::new(&conn)
                .await
                .error("Failed to create LocaleBusProxy")?;
            let mut layout_updates = proxy.receive_layout_changed().await;
            let mut variant_updates = proxy.receive_layout_changed().await;
            loop {
                // zbus does internal caching
                let layout = proxy.layout().await.error("Failed to get layout")?;
                let variant = proxy.variant().await.error("Failed to get layout")?;
                send((layout.into(), Some(variant.into())), &mut api);
                api.flush().await?;
                tokio::select! {
                    _ = layout_updates.next() => (),
                    _ = variant_updates.next() => (),
                }
            }
        }
        KeyboardLayoutDriver::KbddBus => Err(Error::new("Not implemened")),
        // Use sway's IPC to get async updates
        KeyboardLayoutDriver::Sway => {
            let mut connection = Connection::new()
                .await
                .error("Failed to open swayipc connection")?;

            let mut layout = connection
                .get_inputs()
                .await
                .error("failed to get current input")?
                .iter()
                .find_map(|i| {
                    if i.input_type == "keyboard"
                        && config
                            .sway_kb_identifier
                            .as_ref()
                            .map_or(true, |id| id == &i.identifier)
                    {
                        i.xkb_active_layout_name.clone()
                    } else {
                        None
                    }
                })
                .error("Failed to get current input")?;
            send(parse_sway_layout(&layout), &mut api);
            api.flush().await?;

            let mut events = connection
                .subscribe(&[EventType::Input])
                .await
                .error("Failed to subscribe to events")?;
            loop {
                let event = events
                    .next()
                    .await
                    .error("swayipc channel closed")?
                    .error("bad event")?;
                if let Event::Input(event) = event {
                    if let Some(new_layout) = event.input.xkb_active_layout_name {
                        if new_layout != layout {
                            layout = new_layout;
                            send(parse_sway_layout(&layout), &mut api);
                            api.flush().await?;
                        }
                    }
                }
            }
        }
    }
}

fn parse_sway_layout(layout: &str) -> (String, Option<String>) {
    if let Some(i) = layout.find('(') {
        (
            layout[..i].trim_end().into(),
            Some(layout[(i + 1)..].trim_end_matches(')').into()),
        )
    } else {
        (layout.into(), None)
    }
}

#[dbus_proxy(
    interface = "org.freedesktop.locale1",
    default_service = "org.freedesktop.locale1",
    default_path = "/org/freedesktop/locale1"
)]
trait LocaleBus {
    #[dbus_proxy(property, name = "X11Layout")]
    fn layout(&self) -> zbus::Result<StdString>;

    #[dbus_proxy(property, name = "X11Variant")]
    fn variant(&self) -> zbus::Result<StdString>;
}

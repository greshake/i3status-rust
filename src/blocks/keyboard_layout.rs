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

use super::prelude::*;
use std::collections::HashMap;
use swayipc_async::{Connection, Event, EventType};
use tokio::process::Command;
use zbus::dbus_proxy;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(default, deny_unknown_fields)]
struct KeyboardLayoutConfig {
    format: FormatConfig,
    driver: KeyboardLayoutDriver,
    #[default(60.into())]
    interval: Seconds,
    sway_kb_identifier: Option<String>,
    mappings: Option<HashMap<String, String>>,
}

#[derive(Deserialize, Debug, SmartDefault, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum KeyboardLayoutDriver {
    #[default]
    SetXkbMap,
    LocaleBus,
    KbddBus,
    Sway,
}

pub async fn run(config: toml::Value, mut api: CommonApi) -> Result<()> {
    let config = KeyboardLayoutConfig::deserialize(config).config_error()?;
    let mut widget = api
        .new_widget()
        .with_format(config.format.with_default("$layout")?);

    let mut backend: Box<dyn Backend> = match config.driver {
        KeyboardLayoutDriver::SetXkbMap => Box::new(SetXkbMap(config.interval)),
        KeyboardLayoutDriver::LocaleBus => Box::new(LocaleBus::new().await?),
        KeyboardLayoutDriver::KbddBus => return Err(Error::new("KbddBus is not implemented")),
        KeyboardLayoutDriver::Sway => Box::new(Sway::new(config.sway_kb_identifier).await?),
    };

    loop {
        let Info {
            mut layout,
            variant,
        } = backend.get_info().await?;

        let variant = variant.unwrap_or_else(|| "N/A".into());
        if let Some(mappings) = &config.mappings {
            if let Some(mapped) = mappings.get(&format!("{layout} ({variant})")) {
                layout = mapped.clone();
            }
        }

        widget.set_values(map! {
            "layout" => Value::text(layout),
            "variant" => Value::text(variant),
        });

        select! {
            update = backend.wait_for_chagne() => update?,
            UpdateRequest = api.event() => (),
        }
    }
}

#[async_trait]
trait Backend {
    async fn get_info(&mut self) -> Result<Info>;
    async fn wait_for_chagne(&mut self) -> Result<()>;
}

struct Info {
    layout: String,
    variant: Option<String>,
}

struct SetXkbMap(Seconds);

#[async_trait]
impl Backend for SetXkbMap {
    async fn get_info(&mut self) -> Result<Info> {
        let output = Command::new("setxkbmap")
            .arg("-query")
            .output()
            .await
            .error("Failed to execute setxkbmap")?;
        let output =
            String::from_utf8(output.stdout).error("setxkbmap produced a non-UTF8 output")?;
        let layout = output
            .lines()
            // Find the "layout:    xxxx" entry.
            .find(|line| line.starts_with("layout"))
            .error("Could not find the layout entry from setxkbmap")?
            .split_ascii_whitespace()
            .last()
            .error("Could not read the layout entry from setxkbmap.")?;
        Ok(Info {
            layout: layout.into(),
            variant: None,
        })
    }

    async fn wait_for_chagne(&mut self) -> Result<()> {
        sleep(self.0 .0).await;
        Ok(())
    }
}

struct LocaleBus {
    proxy: LocaleBusInterfaceProxy<'static>,
    stream1: zbus::PropertyStream<'static, String>,
    stream2: zbus::PropertyStream<'static, String>,
}

impl LocaleBus {
    async fn new() -> Result<Self> {
        let conn = new_system_dbus_connection().await?;
        let proxy = LocaleBusInterfaceProxy::new(&conn)
            .await
            .error("Failed to create LocaleBusProxy")?;
        let layout_updates = proxy.receive_layout_changed().await;
        let variant_updates = proxy.receive_layout_changed().await;
        Ok(Self {
            proxy,
            stream1: layout_updates,
            stream2: variant_updates,
        })
    }
}

#[async_trait]
impl Backend for LocaleBus {
    async fn get_info(&mut self) -> Result<Info> {
        // zbus does internal caching
        let layout = self.proxy.layout().await.error("Failed to get layout")?;
        let variant = self.proxy.variant().await.error("Failed to get variant")?;
        Ok(Info {
            layout,
            variant: Some(variant),
        })
    }

    async fn wait_for_chagne(&mut self) -> Result<()> {
        select! {
            _ = self.stream1.next() => (),
            _ = self.stream2.next() => (),
        }
        Ok(())
    }
}

struct Sway {
    events: swayipc_async::EventStream,
    cur_layout: String,
    kbd: Option<String>,
}

impl Sway {
    async fn new(kbd: Option<String>) -> Result<Self> {
        let mut connection = Connection::new()
            .await
            .error("Failed to open swayipc connection")?;
        let cur_layout = connection
            .get_inputs()
            .await
            .error("failed to get current input")?
            .iter()
            .find_map(|i| {
                if i.input_type == "keyboard"
                    && kbd.as_deref().map_or(true, |id| id == i.identifier)
                {
                    i.xkb_active_layout_name.clone()
                } else {
                    None
                }
            })
            .error("Failed to get current input")?;
        let events = connection
            .subscribe(&[EventType::Input])
            .await
            .error("Failed to subscribe to events")?;
        Ok(Self {
            events,
            cur_layout,
            kbd,
        })
    }
}

#[async_trait]
impl Backend for Sway {
    async fn get_info(&mut self) -> Result<Info> {
        let (l, v) = parse_sway_layout(&self.cur_layout);
        Ok(Info {
            layout: l,
            variant: v,
        })
    }

    async fn wait_for_chagne(&mut self) -> Result<()> {
        loop {
            let event = self
                .events
                .next()
                .await
                .error("swayipc channel closed")?
                .error("bad event")?;
            if let Event::Input(event) = event {
                if self
                    .kbd
                    .as_deref()
                    .map_or(true, |id| id == event.input.identifier)
                {
                    if let Some(new_layout) = event.input.xkb_active_layout_name {
                        if new_layout != self.cur_layout {
                            self.cur_layout = new_layout;
                            return Ok(());
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
trait LocaleBusInterface {
    #[dbus_proxy(property, name = "X11Layout")]
    fn layout(&self) -> zbus::Result<String>;

    #[dbus_proxy(property, name = "X11Variant")]
    fn variant(&self) -> zbus::Result<String>;
}

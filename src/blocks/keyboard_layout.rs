//! Keyboard layout indicator
//!
//! Six drivers are available:
//! - `xkbevent` which can read asynchronous updates from the x11 events
//! - `setxkbmap` (alias for `xkbevent`) *DEPRECATED*
//! - `xkbswitch` (alias for `xkbevent`) *DEPRECATED*
//! - `localebus` which can read asynchronous updates from the systemd `org.freedesktop.locale1` D-Bus path
//! - `kbddbus` which uses [kbdd](https://github.com/qnikst/kbdd) to monitor per-window layout changes via DBus
//! - `sway` which can read asynchronous updates from the sway IPC
//!
//! `setxkbmap` and `xkbswitch` are deprecated and will be removed in v0.35.0.
//!
//! Which of these methods is appropriate will depend on your system setup.
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `driver` | One of `"xkbevent"`, `"setxkbmap"`, `"xkbswitch"`, `"localebus"`, `"kbddbus"` or `"sway"`, depending on your system. | `"xkbevent"`
//! `interval` *DEPRECATED* | Update interval, in seconds. Only used by the `"setxkbmap"` driver. | `60`
//! `format` | A string to customise the output of this block. See below for available placeholders. | `" $layout "`
//! `sway_kb_identifier` | Identifier of the device you want to monitor, as found in the output of `swaymsg -t get_inputs`. | Defaults to first input found
//! `mappings` | Map `layout (variant)` to custom short name. | `None`
//!
//! `interval` is deprecated and will be removed in v0.35.0.
//!
//!  Key     | Value | Type
//! ---------|-------|-----
//! `layout` | Keyboard layout name | String
//! `variant`| Keyboard variant name or `N/A` if not applicable | String
//!
//! # Examples
//!
//! Listen to D-Bus for changes:
//!
//! ```toml
//! [[block]]
//! block = "keyboard_layout"
//! driver = "localebus"
//! ```
//!
//! Listen to kbdd for changes, the text is in the following format:
//! "English (US)" - {$layout ($variant)}
//! use block.mappings to override with shorter names as shown below.
//! Also use format = " $layout ($variant) " to see the full text to map,
//! or you can use:
//! dbus-monitor interface=ru.gentoo.kbdd
//! to see the exact variant spelling
//!
//! ```toml
//! [[block]]
//! block = "keyboard_layout"
//! driver = "kbddbus"
//! [block.mappings]
//! "English (US)" = "us"
//! "Bulgarian (new phonetic)" = "bg"
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
//! format = " $layout "
//! [block.mappings]
//! "English (Workman)" = "EN"
//! "Russian (N/A)" = "RU"
//! ```
//!
//! Listen to xkb events for changes:
//!
//! ```toml
//! [[block]]
//! block = "keyboard_layout"
//! driver = "xkbevent"
//! ```

mod locale_bus;
use locale_bus::LocaleBus;

mod kbdd_bus;
use kbdd_bus::KbddBus;

mod sway;
use sway::Sway;

mod xkb_event;
use xkb_event::XkbEvent;

use super::prelude::*;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub format: FormatConfig,
    pub driver: KeyboardLayoutDriver,
    #[default(60.into())]
    pub interval: Seconds,
    pub sway_kb_identifier: Option<String>,
    pub mappings: Option<HashMap<String, String>>,
}

#[derive(Deserialize, Debug, SmartDefault, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum KeyboardLayoutDriver {
    #[default]
    XkbEvent,
    SetXkbMap,
    XkbSwitch,
    LocaleBus,
    KbddBus,
    Sway,
}

pub async fn run(config: &Config, api: &CommonApi) -> Result<()> {
    let format = config.format.with_default(" $layout ")?;

    let mut backend: Box<dyn Backend> = match config.driver {
        KeyboardLayoutDriver::LocaleBus => Box::new(LocaleBus::new().await?),
        KeyboardLayoutDriver::KbddBus => Box::new(KbddBus::new().await?),
        KeyboardLayoutDriver::Sway => Box::new(Sway::new(config.sway_kb_identifier.clone()).await?),
        KeyboardLayoutDriver::XkbEvent
        | KeyboardLayoutDriver::SetXkbMap
        | KeyboardLayoutDriver::XkbSwitch => Box::new(XkbEvent::new().await?),
    };

    loop {
        let Info {
            mut layout,
            variant,
        } = backend.get_info().await?;

        let variant = variant.unwrap_or_else(|| "N/A".into());
        if let Some(mappings) = &config.mappings {
            if let Some(mapped) = mappings.get(&format!("{layout} ({variant})")) {
                layout.clone_from(mapped);
            }
        }

        let mut widget = Widget::new().with_format(format.clone());
        widget.set_values(map! {
            "layout" => Value::text(layout),
            "variant" => Value::text(variant),
        });
        api.set_widget(widget)?;

        backend.wait_for_change().await?;
    }
}

#[async_trait]
trait Backend {
    async fn get_info(&mut self) -> Result<Info>;
    async fn wait_for_change(&mut self) -> Result<()>;
}

#[derive(Clone)]
struct Info {
    layout: String,
    variant: Option<String>,
}

impl Info {
    /// Parse "layout (variant)" string
    fn from_layout_variant_str(s: &str) -> Self {
        if let Some((layout, rest)) = s.split_once('(') {
            Self {
                layout: layout.trim_end().into(),
                variant: Some(rest.trim_end_matches(')').into()),
            }
        } else {
            Self {
                layout: s.into(),
                variant: None,
            }
        }
    }
}

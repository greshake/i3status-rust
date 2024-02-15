use std::fmt;

use serde::de::{self, Deserializer, Visitor};
use serde::Deserialize;

use crate::errors::{ErrorContext, Result};
use crate::protocol::i3bar_event::I3BarEvent;
use crate::subprocess::{spawn_shell, spawn_shell_sync};
use crate::wrappers::SerdeRegex;

/// Can be one of `left`, `middle`, `right`, `up`, `down`, `forward`, `back` or `double_left`.
///
/// Note that in order for double clicks to be registered, you have to set `double_click_delay` to a
/// non-zero value. `200` might be a good choice. Note that enabling this functionality will
/// make left clicks less responsive and feel a bit laggy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    WheelUp,
    WheelDown,
    Forward,
    Back,
    DoubleLeft,
}

#[derive(Debug, Clone)]
pub struct PostActions {
    pub action: Option<String>,
    pub update: bool,
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct ClickHandler(Vec<ClickConfigEntry>);

impl ClickHandler {
    pub async fn handle(&self, event: &I3BarEvent) -> Result<Option<PostActions>> {
        let Some(entry) = self
            .0
            .iter()
            .filter(|e| e.button == event.button)
            .find(|e| match &e.widget {
                None => event.instance.is_none(),
                Some(re) => re.0.is_match(event.instance.as_deref().unwrap_or("block")),
            })
        else {
            return Ok(None);
        };

        if let Some(cmd) = &entry.cmd {
            if entry.sync {
                spawn_shell_sync(cmd).await
            } else {
                spawn_shell(cmd)
            }
            .or_error(|| format!("'{:?}' button handler: Failed to run '{cmd}", event.button))?;
        }

        Ok(Some(PostActions {
            action: entry.action.clone(),
            update: entry.update,
        }))
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct ClickConfigEntry {
    /// Which button to handle
    button: MouseButton,
    /// To which part of the block this entry applies
    #[serde(default)]
    widget: Option<SerdeRegex>,
    /// Which command to run
    #[serde(default)]
    cmd: Option<String>,
    /// Which block action to trigger
    #[serde(default)]
    action: Option<String>,
    /// Whether to wait for command to exit or not (default is `false`)
    #[serde(default)]
    sync: bool,
    /// Whether to update the block on click (default is `false`)
    #[serde(default)]
    update: bool,
}

impl<'de> Deserialize<'de> for MouseButton {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct MouseButtonVisitor;

        impl<'de> Visitor<'de> for MouseButtonVisitor {
            type Value = MouseButton;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("button as int or string")
            }

            // ```toml
            // button = "left"
            // ```
            fn visit_str<E>(self, name: &str) -> Result<MouseButton, E>
            where
                E: de::Error,
            {
                use MouseButton::*;
                Ok(match name {
                    "left" => Left,
                    "middle" => Middle,
                    "right" => Right,
                    "up" => WheelUp,
                    "down" => WheelDown,
                    "forward" => Forward,
                    "back" => Back,
                    // Experimental
                    "double_left" => DoubleLeft,
                    other => return Err(E::custom(format!("unknown button '{other}'"))),
                })
            }

            // ```toml
            // button = 1
            // ```
            fn visit_i64<E>(self, number: i64) -> Result<MouseButton, E>
            where
                E: de::Error,
            {
                use MouseButton::*;
                Ok(match number {
                    1 => Left,
                    2 => Middle,
                    3 => Right,
                    4 => WheelUp,
                    5 => WheelDown,
                    8 => Back,
                    9 => Forward,
                    other => return Err(E::custom(format!("unknown button '{other}'"))),
                })
            }
            fn visit_u64<E>(self, number: u64) -> Result<MouseButton, E>
            where
                E: de::Error,
            {
                self.visit_i64(number as i64)
            }
        }

        deserializer.deserialize_any(MouseButtonVisitor)
    }
}

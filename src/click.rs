use std::fmt;

use serde::de::{self, Deserialize, Deserializer, Visitor};

use crate::errors::{self, ResultExt};
use crate::subprocess::{spawn_shell, spawn_shell_sync};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    WheelUp,
    WheelDown,
    Forward,
    Back,
    Unknown,
    /// Experemental
    DoubleLeft,
}

#[derive(serde_derive::Deserialize, Debug, Clone, Default)]
pub struct ClickHandler(Vec<ClickConfigEntry>);

impl ClickHandler {
    // Returns true if the block needs to be updated
    pub async fn handle(&self, button: MouseButton) -> errors::Result<bool> {
        Ok(match self.0.iter().find(|e| e.button == button) {
            Some(entry) => {
                if let Some(cmd) = &entry.cmd {
                    if entry.sync {
                        spawn_shell_sync(cmd).await
                    } else {
                        spawn_shell(cmd)
                    }
                    .or_error(|| {
                        format!("'{:?}' button handler: Failed to run '{}", button, cmd)
                    })?;
                }
                entry.update
            }
            None => true,
        })
    }
}

#[derive(serde_derive::Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct ClickConfigEntry {
    /// Which button to handle
    button: MouseButton,
    /// Which command to run
    #[serde(default)]
    cmd: Option<String>,
    /// Whether to wait for command to exit or not (default is `false`)
    #[serde(default)]
    sync: bool,
    /// Whether to update the block on click (default is `true`)
    #[serde(default = "return_true")]
    update: bool,
}

fn return_true() -> bool {
    true
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
                formatter.write_str("u64 or string")
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
                    // Experemental
                    "double_left" => DoubleLeft,
                    _ => Unknown,
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
                    9 => Forward,
                    8 => Back,
                    _ => Unknown,
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

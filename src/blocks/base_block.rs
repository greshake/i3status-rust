//! A Base block for common behavior for all blocks

use serde_derive::Deserialize;
use std::collections::HashMap;
use toml::{value::Table, Value};

#[derive(Deserialize, Debug, Default, Clone)]
pub(super) struct BaseBlockConfig {
    /// Command to execute when the button is clicked
    pub on_click: Option<String>,
    /// Signal to update upon reception
    pub signal: Option<i32>,

    pub theme_overrides: Option<HashMap<String, String>>,
    pub icons_overrides: Option<HashMap<String, String>>,
    pub icons_format: Option<String>,
    pub if_command: Option<String>,
}

impl BaseBlockConfig {
    const FIELDS: &'static [&'static str] = &[
        "on_click",
        "signal",
        "theme_overrides",
        "icons_overrides",
        "icons_format",
        "if_command",
    ];

    // FIXME: this function is to paper over https://github.com/serde-rs/serde/issues/1957
    pub(super) fn extract(config: &mut Value) -> Value {
        let mut common_table = Table::new();
        if let Some(table) = config.as_table_mut() {
            for &field in Self::FIELDS {
                if let Some(it) = table.remove(field) {
                    common_table.insert(field.to_string(), it);
                }
            }
        }
        common_table.into()
    }
}

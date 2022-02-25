//! A Base block for common behavior for all blocks

use std::collections::HashMap;

use crate::errors::*;
use crate::protocol::i3bar_event::{I3BarEvent, MouseButton};
use crate::{blocks::Update, subprocess::spawn_child_async, widgets::I3BarWidget, Block};

use serde_derive::Deserialize;
use toml::{value::Table, Value};

pub(super) struct BaseBlock<T: Block> {
    pub name: String,
    pub inner: T,
    pub on_click: Option<String>,
}

impl<T: Block> Block for BaseBlock<T> {
    fn id(&self) -> usize {
        self.inner.id()
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        self.inner.view()
    }

    fn update(&mut self) -> Result<Option<Update>> {
        self.inner.update()
    }

    fn signal(&mut self, signal: i32) -> Result<()> {
        self.inner.signal(signal)
    }

    fn click(&mut self, e: &I3BarEvent) -> Result<()> {
        match &self.on_click {
            Some(cmd) => {
                if let MouseButton::Left = e.button {
                    spawn_child_async("sh", &["-c", cmd])
                        .block_error(&self.name, "could not spawn child")?;
                }
                Ok(())
            }
            None => self.inner.click(e),
        }
    }
}

#[derive(Deserialize, Debug, Default, Clone)]
pub(super) struct BaseBlockConfig {
    /// Command to execute when the button is clicked
    pub on_click: Option<String>,

    pub theme_overrides: Option<HashMap<String, String>>,
    pub icons_overrides: Option<HashMap<String, String>>,
    pub icons_format: Option<String>,
    pub if_command: Option<String>,
}

impl BaseBlockConfig {
    const FIELDS: &'static [&'static str] = &[
        "on_click",
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

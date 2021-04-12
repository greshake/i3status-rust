//! A Base block for common behavior for all blocks

use std::collections::HashMap;

use crate::errors::*;
use crate::protocol::i3bar_event::{I3BarEvent, MouseButton};
use crate::{blocks::Update, widgets::I3BarWidget, Block};

use async_trait::async_trait;
use serde_derive::Deserialize;
use tokio::process::Command;
use toml::{value::Table, Value};

pub(super) struct BaseBlock<T: Block> {
    pub name: String,
    pub inner: T,
    pub on_click: Option<String>,
}

#[async_trait(?Send)]
impl<T: Block> Block for BaseBlock<T> {
    fn id(&self) -> usize {
        self.inner.id()
    }

    async fn render(&mut self) -> Result<Vec<Box<dyn I3BarWidget>>> {
        self.inner.render().await
    }

    fn update_interval(&self) -> Update {
        self.inner.update_interval()
    }

    async fn signal(&mut self, signal: i32) -> Result<bool> {
        self.inner.signal(signal).await
    }

    async fn click(&mut self, e: I3BarEvent) -> Result<bool> {
        match &self.on_click {
            Some(cmd) => {
                if let MouseButton::Left = e.button {
                    Command::new("sh")
                        .args(&["-c", &cmd])
                        .spawn()
                        .block_error(&self.name, "could not spawn child")?;
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            None => self.inner.click(e).await,
        }
    }
}

#[derive(Deserialize, Debug, Default, Clone)]
pub(super) struct BaseBlockConfig {
    /// Command to execute when the button is clicked
    pub on_click: Option<String>,

    pub theme_overrides: Option<HashMap<String, String>>,
    pub icons_format: Option<String>,
}

impl BaseBlockConfig {
    const FIELDS: &'static [&'static str] = &["on_click", "theme_overrides", "icons_format"];

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

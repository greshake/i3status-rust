//! A Base block for common behavior for all blocks

use crate::errors::*;
use crate::{
    blocks::Update,
    input::{I3BarEvent, MouseButton},
    subprocess::spawn_child_async,
    widget::I3BarWidget,
    Block,
};
use serde_derive::Deserialize;

pub(super) struct BaseBlock<T: Block> {
    pub name: String,
    pub inner: T,
    pub on_click: Option<String>,
}

impl<T: Block> Block for BaseBlock<T> {
    fn id(&self) -> &str {
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
                if let Some(ref name) = e.name {
                    if name.as_str() == self.id() {
                        if let MouseButton::Left = e.button {
                            spawn_child_async("sh", &["-c", &cmd])
                                .block_error(&self.name, "could not spawn child")?;
                        }
                    }
                }
                Ok(())
            }
            None => self.inner.click(e),
        }
    }
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub(super) struct BaseBlockConfig<T> {
    /// Command to execute when the button is clicked
    pub on_click: Option<String>,

    #[serde(flatten)]
    pub inner: T,
}

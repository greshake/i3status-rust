//! A custom menu
//!
//! This block allows you to quickly run a custom shell command. Left-click on this block to
//! activate it, then scroll through configured items. Left-click on the item to run it and
//! optionally confirm your action by left-clicking again. Right-click any time to deactivate this
//! block.
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `text` | Text that will be displayed when the block is inactive. | **Required**
//! `items` | A list of "items". See examples below. | **Required**
//!
//! # Example
//!
//! ```toml
//! [[block]]
//! block = "menu"
//! text = "\uf011"
//! [[block.items]]
//! display = " -&gt;   Sleep   &lt;-"
//! cmd = "systemctl suspend"
//! [[block.items]]
//! display = " -&gt; Power Off &lt;-"
//! cmd = "poweroff"
//! confirm_msg = "Are you sure you want to power off?"
//! [[block.items]]
//! display = " -&gt;  Reboot   &lt;-"
//! cmd = "reboot"
//! confirm_msg = "Are you sure you want to reboot?"
//! ```

use super::prelude::*;
use crate::subprocess::spawn_shell;

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Config {
    text: String,
    items: Vec<Item>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
struct Item {
    display: String,
    cmd: String,
    #[serde(default)]
    confirm_msg: Option<String>,
}

struct Block {
    api: CommonApi,
    text: String,
    items: Vec<Item>,
}

impl Block {
    async fn reset(&mut self) -> Result<()> {
        self.set_text(self.text.clone()).await
    }

    async fn set_text(&mut self, text: String) -> Result<()> {
        self.api.set_widget(Widget::new().with_text(text)).await
    }

    async fn wait_for_click(&mut self, button: &str) {
        loop {
            match self.api.event().await {
                Action(a) if a == button => break,
                _ => (),
            }
        }
    }

    async fn run_menu(&mut self) -> Result<Option<Item>> {
        let mut index = 0;
        loop {
            self.set_text(self.items[index].display.clone()).await?;
            if let Action(action) = self.api.event().await {
                match action.as_ref() {
                    "_up" => index += 1,
                    "_down" => index += self.items.len() + 1,
                    "_left" => return Ok(Some(self.items[index].clone())),
                    "_right" => return Ok(None),
                    _ => (),
                }
            }
            index %= self.items.len();
        }
    }

    async fn confirm(&mut self, msg: String) -> Result<bool> {
        self.set_text(msg).await?;
        loop {
            if let Action(action) = self.api.event().await {
                return Ok(action == "_left");
            }
        }
    }
}

pub async fn run(config: Config, mut api: CommonApi) -> Result<()> {
    api.set_default_actions(&[
        (MouseButton::Left, None, "_left"),
        (MouseButton::Right, None, "_right"),
        (MouseButton::WheelUp, None, "_up"),
        (MouseButton::WheelDown, None, "_down"),
    ])
    .await?;

    let mut block = Block {
        api,
        text: config.text,
        items: config.items,
    };

    loop {
        block.reset().await?;
        block.wait_for_click("_left").await;
        if let Some(res) = block.run_menu().await? {
            if let Some(msg) = res.confirm_msg {
                if !block.confirm(msg).await? {
                    continue;
                }
            }
            spawn_shell(&res.cmd).or_error(|| format!("Failed to run '{}'", res.cmd))?;
        }
    }
}

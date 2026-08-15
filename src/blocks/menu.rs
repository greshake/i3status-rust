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

use tokio::sync::mpsc::UnboundedReceiver;

use super::{BlockAction, prelude::*};
use crate::subprocess::spawn_shell;

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub text: String,
    pub items: Vec<Item>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct Item {
    pub display: String,
    pub cmd: String,
    #[serde(default)]
    pub confirm_msg: Option<String>,
}

struct Block<'a> {
    actions: UnboundedReceiver<BlockAction>,
    api: &'a CommonApi,
    output: OutputHandle,
    text: &'a str,
    items: &'a [Item],
}

impl Block<'_> {
    async fn reset(&mut self) -> Result<()> {
        self.set_text(self.text.to_owned()).await
    }

    async fn set_text(&mut self, text: String) -> Result<()> {
        self.api.set_widget(self.output.new_text_widget(text))
    }

    async fn wait_for_click(&mut self, button: &str) -> Result<()> {
        while self.actions.recv().await.error("channel closed")? != button {}
        Ok(())
    }

    async fn run_menu(&mut self) -> Result<Option<Item>> {
        let mut index = 0;
        loop {
            self.set_text(self.items[index].display.clone()).await?;
            match &*self.actions.recv().await.error("channel closed")? {
                "_up" => index += 1,
                "_down" => index += self.items.len() + 1,
                "_left" => return Ok(Some(self.items[index].clone())),
                "_right" => return Ok(None),
                _ => (),
            }
            index %= self.items.len();
        }
    }

    async fn confirm(&mut self, msg: String) -> Result<bool> {
        self.set_text(msg).await?;
        Ok(self.actions.recv().await.as_deref() == Some("_left"))
    }
}

pub(crate) fn prepare(_config: &Config) -> Result<Arc<BlockPlan>> {
    // The bar text is the configured label, a menu item's display string or a
    // confirmation message — plain text with no placeholders and no icons, so
    // the block declares a single text output rather than a format.
    BlockPlan::new(vec![OutputPlan::text("main")])
}

pub(crate) async fn run(config: &Config, api: &CommonApi, plan: &Arc<BlockPlan>) -> Result<()> {
    api.set_default_actions(&[
        (MouseButton::Left, None, "_left"),
        (MouseButton::Right, None, "_right"),
        (MouseButton::WheelUp, None, "_up"),
        (MouseButton::WheelDown, None, "_down"),
    ])?;

    let mut block = Block {
        actions: api.get_actions()?,
        api,
        output: plan.output("main")?,
        text: &config.text,
        items: &config.items,
    };

    loop {
        block.reset().await?;
        block.wait_for_click("_left").await?;
        if let Some(res) = block.run_menu().await? {
            if let Some(msg) = res.confirm_msg
                && !block.confirm(msg).await?
            {
                continue;
            }
            spawn_shell(&res.cmd).or_error(|| format!("Failed to run '{}'", res.cmd))?;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::block_plan::OutputKind;

    fn config() -> Config {
        Config {
            text: "menu".into(),
            items: Vec::new(),
        }
    }

    #[test]
    fn plan_declares_one_text_output_and_no_icons() {
        let plan = prepare(&config()).unwrap();
        let ids: Vec<_> = plan.outputs().map(|o| o.id()).collect();
        assert_eq!(ids, ["main"]);

        let output = plan.output("main").unwrap();
        assert_eq!(output.output().kind(), OutputKind::Text);
        assert_eq!(output.output().icon_placeholders().count(), 0);
    }

    #[test]
    fn every_text_this_block_renders_carries_the_contract() {
        let plan = prepare(&config()).unwrap();
        let output = plan.output("main").unwrap();
        // The three strings the block can put on the bar: the label, a menu
        // item's display string and a confirmation message.
        for text in ["menu", "Shut down", "Are you sure?"] {
            let widget = output.new_text_widget(text.into());
            assert!(widget.contract().is_some(), "{text}");
            widget.check_contract().unwrap();
        }
    }
}

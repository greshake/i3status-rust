use super::prelude::*;
use crate::subprocess::spawn_shell;
use tokio::sync::mpsc;

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct Config {
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
    events_receiver: mpsc::Receiver<BlockEvent>,
}

impl Block {
    async fn reset(&mut self) -> Result<()> {
        self.set_text(self.text.clone()).await
    }

    async fn set_text(&mut self, text: String) -> Result<()> {
        self.api.set_text(text);
        self.api.flush().await
    }

    async fn wait_for_click(&mut self, button: MouseButton) {
        loop {
            match self.events_receiver.recv().await.unwrap() {
                BlockEvent::Click(c) if c.button == button => break,
                _ => (),
            }
        }
    }

    async fn run_menu(&mut self) -> Result<Option<Item>> {
        let mut index = 0;
        loop {
            self.set_text(self.items[index].display.clone()).await?;

            if let BlockEvent::Click(c) = self.events_receiver.recv().await.unwrap() {
                match c.button {
                    MouseButton::WheelUp => index += 1,
                    MouseButton::WheelDown => index += self.items.len() + 1,
                    MouseButton::Left => return Ok(Some(self.items[index].clone())),
                    MouseButton::Right => return Ok(None),
                    _ => (),
                }
            }
            index %= self.items.len();
        }
    }

    async fn confirm(&mut self, msg: String) -> Result<bool> {
        self.set_text(msg).await?;
        loop {
            if let BlockEvent::Click(c) = self.events_receiver.recv().await.unwrap() {
                return Ok(c.button == MouseButton::DoubleLeft);
            }
        }
    }
}

pub async fn run(config: toml::Value, mut api: CommonApi) -> Result<()> {
    let events = api.get_events().await?;
    let config = Config::deserialize(config).config_error()?;

    let mut block = Block {
        api,
        text: config.text,
        items: config.items,
        events_receiver: events,
    };

    loop {
        block.reset().await?;
        block.wait_for_click(MouseButton::Left).await;
        if let Some(res) = block.run_menu().await? {
            if let Some(msg) = res.confirm_msg {
                if !block.confirm(msg.clone()).await? {
                    continue;
                }
            }
            spawn_shell(&res.cmd).or_error(|| format!("Failed to run '{}'", res.cmd))?;
        }
    }
}

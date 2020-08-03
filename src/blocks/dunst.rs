use crossbeam_channel::Sender;
use serde_derive::Deserialize;
use uuid::Uuid;

use crate::blocks::{Block, ConfigBlock};
use crate::config::Config;
use crate::errors::*;
use crate::input::{I3BarEvent, MouseButton};
use crate::scheduler::Task;
use crate::subprocess::spawn_child_async;
use crate::widget::I3BarWidget;
use crate::widgets::button::ButtonWidget;

pub struct Dunst {
    icon: ButtonWidget,
    id: String,
    paused: bool,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct DunstConfig {}

impl DunstConfig {}

impl ConfigBlock for Dunst {
    type Config = DunstConfig;

    fn new(
        _block_config: Self::Config,
        config: Config,
        _tx_update_request: Sender<Task>,
    ) -> Result<Self> {
        // make sure that dunst is currently running
        spawn_child_async("sh", &["-c", "killall -s SIGUSR2 dunst"])
            .block_error("dunst", "could not spawn child")?;

        let i = Uuid::new_v4().to_simple().to_string();

        Ok(Dunst {
            id: i.clone(),
            icon: ButtonWidget::new(config, i.as_str()).with_icon("bell"),
            paused: false,
        })
    }
}

impl Block for Dunst {
    fn click(&mut self, e: &I3BarEvent) -> Result<()> {
        if let Some(ref name) = e.name {
            if name.as_str() == self.id {
                if let MouseButton::Left = e.button {
                    self.paused = !self.paused;
                    if self.paused {
                        spawn_child_async("sh", &["-c", "killall -s SIGUSR1 dunst"])
                            .block_error("dunst", "could not spawn child")?;
                    } else {
                        spawn_child_async("sh", &["-c", "killall -s SIGUSR2 dunst"])
                            .block_error("dunst", "could not spawn child")?;
                    }
                    let icon = if self.paused { "bell-slash" } else { "bell" };
                    self.icon.set_icon(icon);
                }
            }
        }
        Ok(())
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.icon]
    }

    fn id(&self) -> &str {
        &self.id
    }
}

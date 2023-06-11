// use super::super::prelude::*;
use super::*;
use swayipc_async::{Connection, Event, EventType};

pub(super) struct Sway {
    events: swayipc_async::EventStream,
    cur_layout: String,
    kbd: Option<String>,
}

impl Sway {
    pub(super) async fn new(kbd: Option<String>) -> Result<Self> {
        let mut connection = Connection::new()
            .await
            .error("Failed to open swayipc connection")?;
        let cur_layout = connection
            .get_inputs()
            .await
            .error("failed to get current input")?
            .iter()
            .find_map(|i| {
                if i.input_type == "keyboard"
                    && kbd.as_deref().map_or(true, |id| id == i.identifier)
                {
                    i.xkb_active_layout_name.clone()
                } else {
                    None
                }
            })
            .error("Failed to get current input")?;
        let events = connection
            .subscribe(&[EventType::Input])
            .await
            .error("Failed to subscribe to events")?;
        Ok(Self {
            events,
            cur_layout,
            kbd,
        })
    }
}

#[async_trait]
impl Backend for Sway {
    async fn get_info(&mut self) -> Result<Info> {
        Ok(Info::from_layout_variant_str(&self.cur_layout))
    }

    async fn wait_for_change(&mut self) -> Result<()> {
        loop {
            let event = self
                .events
                .next()
                .await
                .error("swayipc channel closed")?
                .error("bad event")?;
            if let Event::Input(event) = event {
                if self
                    .kbd
                    .as_deref()
                    .map_or(true, |id| id == event.input.identifier)
                {
                    if let Some(new_layout) = event.input.xkb_active_layout_name {
                        if new_layout != self.cur_layout {
                            self.cur_layout = new_layout;
                            return Ok(());
                        }
                    }
                }
            }
        }
    }
}

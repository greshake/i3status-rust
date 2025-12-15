use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

use super::*;
use crate::pipewire::{CLIENT, CommandKind, EventKind, PwSender};

pub(super) struct Device {
    device_kind: DeviceKind,
    match_name: Option<String>,
    id: Option<u32>,
    name: String,
    description: Option<String>,
    active_port: Option<String>,
    form_factor: Option<String>,
    volume: Vec<f32>,
    volume_avg: u32,
    muted: bool,
    updates: UnboundedReceiver<EventKind>,
    command_sender: PwSender<CommandKind>,
}

impl Device {
    pub(super) async fn new(device_kind: DeviceKind, match_name: Option<String>) -> Result<Self> {
        let client = CLIENT.as_ref().error("Could not get client")?;

        let (tx, rx) = unbounded_channel();
        client.add_event_listener(tx);
        let command_sender = client.add_command_listener();
        let mut s = Self {
            device_kind,
            match_name,
            id: None,
            name: "".into(),
            description: None,
            active_port: None,
            form_factor: None,
            volume: Vec::new(),
            volume_avg: 0,
            muted: false,
            updates: rx,
            command_sender,
        };
        s.get_info().await?;
        Ok(s)
    }
}

#[async_trait]
impl SoundDevice for Device {
    fn volume(&self) -> u32 {
        self.volume_avg
    }

    fn muted(&self) -> bool {
        self.muted
    }

    fn output_name(&self) -> String {
        self.name.clone()
    }

    fn output_description(&self) -> Option<String> {
        self.description.clone()
    }

    fn active_port(&self) -> Option<String> {
        self.active_port.clone()
    }

    fn form_factor(&self) -> Option<&str> {
        self.form_factor.as_deref()
    }

    async fn get_info(&mut self) -> Result<()> {
        let client = CLIENT.as_ref().error("Could not get client")?;
        let data = client.data.lock().unwrap();

        let name = if self.match_name.is_some() {
            // If name is specified in the config, then match that node
            &self.match_name
        } else {
            // Otherwise use the default metadata to determine the node name
            match self.device_kind {
                DeviceKind::Sink => &data.default_metadata.sink,
                DeviceKind::Source => &data.default_metadata.source,
            }
        };

        let Some(name) = name else {
            //Metadata may not be ready yet
            return Ok(());
        };

        if let Some((id, node)) = data.nodes.iter().find(|(_, node)| node.name == *name) {
            self.id = Some(*id);
            if let Some(volume) = &node.volume {
                self.volume = volume.clone();
                self.volume_avg = (volume.iter().sum::<f32>() / volume.len() as f32).round() as u32;
            }
            if let Some(muted) = node.muted {
                self.muted = muted;
            }
            self.name = node.name.clone();
            self.description = node.description.clone();
            self.form_factor = node.form_factor.clone();

            if let Some(device_id) = node.device_id
                && let Some(direction) = node.direction
                && let Some(directed_routes) = data.directed_routes.get(&device_id)
                && let Some(route) = directed_routes.get_route(direction)
            {
                self.active_port = Some(route.name.clone());
            }
        } else {
            self.id = None;
        }

        Ok(())
    }

    async fn set_volume(&mut self, step: i32, max_vol: Option<u32>) -> Result<()> {
        if let Some(id) = self.id {
            let volume = self
                .volume
                .iter()
                .map(|&vol| {
                    let uncapped_vol = 0_f32.max(vol + step as f32);
                    if let Some(vol_cap) = max_vol {
                        uncapped_vol.min(vol_cap as f32)
                    } else {
                        uncapped_vol
                    }
                })
                .collect();

            self.command_sender
                .send(CommandKind::SetVolume(id, volume))
                .map_err(|_| Error::new("Could not set volume"))?;
        }
        Ok(())
    }

    async fn toggle(&mut self) -> Result<()> {
        if let Some(id) = self.id {
            self.command_sender
                .send(CommandKind::Mute(id, !self.muted))
                .map_err(|_| Error::new("Could not toggle mute"))?;
        }
        Ok(())
    }

    async fn wait_for_update(&mut self) -> Result<()> {
        while let Some(event) = self.updates.recv().await {
            if event.intersects(
                EventKind::DEFAULT_META_DATA_UPDATED
                    | EventKind::DEVICE_ADDED
                    | EventKind::DEVICE_PARAM_UPDATE
                    | EventKind::DEVICE_REMOVED
                    | EventKind::NODE_PARAM_UPDATE
                    | EventKind::NODE_STATE_UPDATE
                    | EventKind::PORT_ADDED
                    | EventKind::PORT_REMOVED,
            ) {
                break;
            }
        }
        Ok(())
    }
}

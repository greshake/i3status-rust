use itertools::Itertools as _;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

use super::*;
use crate::pipewire::{CLIENT, EventKind, Link, Node};

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(rename_all = "lowercase", deny_unknown_fields, default)]
pub struct Config {
    exclude_output: Vec<String>,
    exclude_input: Vec<String>,
    display: NodeDisplay,
}

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(rename_all = "snake_case")]
enum NodeDisplay {
    #[default]
    Name,
    Description,
    Nickname,
}

impl NodeDisplay {
    fn map_node(&self, node: &Node) -> String {
        match self {
            NodeDisplay::Name => node.name.clone(),
            NodeDisplay::Description => node.description.clone().unwrap_or(node.name.clone()),
            NodeDisplay::Nickname => node.nick.clone().unwrap_or(node.name.clone()),
        }
    }
}

pub(super) struct Monitor<'a> {
    config: &'a Config,
    updates: UnboundedReceiver<EventKind>,
}

impl<'a> Monitor<'a> {
    pub(super) async fn new(config: &'a Config) -> Result<Self> {
        let client = CLIENT.as_ref().error("Could not get client")?;
        let (tx, rx) = unbounded_channel();
        client.add_event_listener(tx);
        Ok(Self {
            config,
            updates: rx,
        })
    }
}

#[async_trait]
impl PrivacyMonitor for Monitor<'_> {
    async fn get_info(&mut self) -> Result<PrivacyInfo> {
        let client = CLIENT.as_ref().error("Could not get client")?;
        let data = client.data.lock().unwrap();
        let mut mapping: PrivacyInfo = PrivacyInfo::new();

        for node in data.nodes.values() {
            debug! {"{:?}", node};
        }

        // The links must be sorted and then dedup'ed since you can multiple links between any given pair of nodes
        for Link {
            link_output_node,
            link_input_node,
            ..
        } in data.links.values().sorted().dedup()
        {
            if let Some(output_node) = data.nodes.get(link_output_node)
                && let Some(input_node) = data.nodes.get(link_input_node)
                && input_node.media_class != Some("Audio/Sink".into())
                && !self.config.exclude_output.contains(&output_node.name)
                && !self.config.exclude_input.contains(&input_node.name)
            {
                let type_ = if input_node.media_class == Some("Stream/Input/Video".into()) {
                    if output_node.media_role == Some("Camera".into()) {
                        Type::Webcam
                    } else {
                        Type::Video
                    }
                } else if input_node.media_class == Some("Stream/Input/Audio".into()) {
                    if output_node.media_class == Some("Audio/Sink".into()) {
                        Type::AudioSink
                    } else {
                        Type::Audio
                    }
                } else {
                    Type::Unknown
                };
                *mapping
                    .entry(type_)
                    .or_default()
                    .entry(self.config.display.map_node(output_node))
                    .or_default()
                    .entry(self.config.display.map_node(input_node))
                    .or_default() += 1;
            }
        }

        Ok(mapping)
    }

    async fn wait_for_change(&mut self) -> Result<()> {
        while let Some(event) = self.updates.recv().await {
            if event.intersects(
                EventKind::NODE_ADDED
                    | EventKind::NODE_REMOVED
                    | EventKind::LINK_ADDED
                    | EventKind::LINK_REMOVED,
            ) {
                break;
            }
        }
        Ok(())
    }
}

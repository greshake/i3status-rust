use ::pipewire::{
    context::Context, core::PW_ID_CORE, keys, main_loop::MainLoop, properties::properties,
    spa::utils::dict::DictRef, types::ObjectType,
};
use itertools::Itertools;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

use std::{collections::HashMap, sync::Mutex, thread};

use super::*;

static CLIENT: Lazy<Result<Client>> = Lazy::new(Client::new);

#[derive(Debug)]
struct Node {
    name: String,
    nick: Option<String>,
    media_class: Option<String>,
    media_role: Option<String>,
    description: Option<String>,
}

impl Node {
    fn new(global_id: u32, global_props: &DictRef) -> Self {
        Self {
            name: global_props
                .get(&keys::NODE_NAME)
                .map_or_else(|| format!("node_{}", global_id), |s| s.to_string()),
            nick: global_props.get(&keys::NODE_NICK).map(|s| s.to_string()),
            media_class: global_props.get(&keys::MEDIA_CLASS).map(|s| s.to_string()),
            media_role: global_props.get(&keys::MEDIA_ROLE).map(|s| s.to_string()),
            description: global_props
                .get(&keys::NODE_DESCRIPTION)
                .map(|s| s.to_string()),
        }
    }
}

#[derive(Debug, PartialEq, PartialOrd, Eq, Ord)]
struct Link {
    link_output_node: u32,
    link_input_node: u32,
}

impl Link {
    fn new(global_props: &DictRef) -> Option<Self> {
        if let (Some(link_output_node), Some(link_input_node)) = (
            global_props
                .get(&keys::LINK_OUTPUT_NODE)
                .and_then(|s| s.parse().ok()),
            global_props
                .get(&keys::LINK_INPUT_NODE)
                .and_then(|s| s.parse().ok()),
        ) {
            Some(Self {
                link_output_node,
                link_input_node,
            })
        } else {
            None
        }
    }
}

#[derive(Default)]
struct Data {
    nodes: HashMap<u32, Node>,
    links: HashMap<u32, Link>,
}

#[derive(Default)]
struct Client {
    event_listeners: Mutex<Vec<UnboundedSender<()>>>,
    ready: Mutex<bool>,
    data: Mutex<Data>,
}

impl Client {
    fn new() -> Result<Client> {
        thread::Builder::new()
            .name("privacy_pipewire".to_string())
            .spawn(Client::main_loop_thread)
            .error("failed to spawn a thread")?;

        Ok(Client::default())
    }

    fn main_loop_thread() {
        let client = CLIENT.as_ref().error("Could not get client").unwrap();

        let proplist = properties! {*keys::APP_NAME => env!("CARGO_PKG_NAME")};

        let main_loop = MainLoop::new(None).expect("Failed to create main loop");

        let context =
            Context::with_properties(&main_loop, proplist).expect("Failed to create context");
        let core = context.connect(None).expect("Failed to connect");
        let registry = core.get_registry().expect("Failed to get registry");

        // Trigger the sync event. The server's answer won't be processed until we start the main loop,
        // so we can safely do this before setting up a callback. This lets us avoid using a Cell.
        let pending = core.sync(0).expect("sync failed");

        let _core_listener = core
            .add_listener_local()
            .done(move |id, seq| {
                if id == PW_ID_CORE && seq == pending {
                    debug!("ready");
                    *client.ready.lock().unwrap() = true;
                }
            })
            .register();

        // Register a callback to the `global` event on the registry, which notifies of any new global objects
        // appearing on the remote.
        // The callback will only get called as long as we keep the returned listener alive.
        let _registry_listener = registry
            .add_listener_local()
            .global(move |global| {
                let global_id = global.id;
                let Some(global_props) = global.props else {
                    return;
                };
                if global.type_ == ObjectType::Node {
                    client
                        .data
                        .lock()
                        .unwrap()
                        .nodes
                        .insert(global_id, Node::new(global_id, global_props));
                    client.send_update_event();
                } else if global.type_ == ObjectType::Link {
                    let Some(link) = Link::new(global_props) else {
                        return;
                    };
                    client.data.lock().unwrap().links.insert(global_id, link);
                    client.send_update_event();
                }
            })
            .global_remove(move |uid| {
                let mut data = client.data.lock().unwrap();
                if data.nodes.remove(&uid).is_some() {
                    client.send_update_event();
                }
                if data.links.remove(&uid).is_some() {
                    client.send_update_event();
                }
            })
            .register();

        main_loop.run();
    }

    fn add_event_listener(&self, tx: UnboundedSender<()>) {
        self.event_listeners.lock().unwrap().push(tx);
    }

    fn send_update_event(&self) {
        self.event_listeners
            .lock()
            .unwrap()
            .retain(|tx| tx.send(()).is_ok());
    }

    async fn wait_until_ready(&self) {
        while !*self.ready.lock().unwrap() {
            sleep(Duration::from_millis(1)).await;
        }
    }
}

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
    updates: UnboundedReceiver<()>,
}

impl<'a> Monitor<'a> {
    pub(super) async fn new(config: &'a Config) -> Result<Self> {
        let client = CLIENT.as_ref().error("Could not get client")?;
        client.wait_until_ready().await;

        let (tx, rx) = mpsc::unbounded_channel();
        client.add_event_listener(tx);
        Ok(Self {
            config,
            updates: rx,
        })
    }
}

#[async_trait]
impl<'a> PrivacyMonitor for Monitor<'a> {
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
            let (Some(output_node), Some(input_node)) = (
                data.nodes.get(link_output_node),
                data.nodes.get(link_input_node),
            ) else {
                continue;
            };
            if input_node.media_class != Some("Audio/Sink".into())
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
        self.updates.recv().await;
        Ok(())
    }
}

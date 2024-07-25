use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex, Weak};
use std::thread;

use ::pipewire::{
    context::Context, keys, main_loop::MainLoop, properties::properties, spa::utils::dict::DictRef,
    types::ObjectType,
};
use itertools::Itertools;
use tokio::sync::Notify;

use super::*;

static CLIENT: LazyLock<Result<Client>> = LazyLock::new(Client::new);

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
    event_listeners: Mutex<Vec<Weak<Notify>>>,
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

        let updated = Rc::new(Cell::new(false));
        let updated_copy = updated.clone();
        let updated_copy2 = updated.clone();

        // Register a callback to the `global` event on the registry, which notifies of any new global objects
        // appearing on the remote.
        // The callback will only get called as long as we keep the returned listener alive.
        let _registry_listener = registry
            .add_listener_local()
            .global(move |global| {
                let Some(global_props) = global.props else {
                    return;
                };
                match &global.type_ {
                    ObjectType::Node => {
                        client
                            .data
                            .lock()
                            .unwrap()
                            .nodes
                            .insert(global.id, Node::new(global.id, global_props));
                        updated_copy.set(true);
                    }
                    ObjectType::Link => {
                        let Some(link) = Link::new(global_props) else {
                            return;
                        };
                        client.data.lock().unwrap().links.insert(global.id, link);
                        updated_copy.set(true);
                    }
                    _ => (),
                }
            })
            .global_remove(move |uid| {
                let mut data = client.data.lock().unwrap();
                if data.nodes.remove(&uid).is_some() || data.links.remove(&uid).is_some() {
                    updated_copy2.set(true);
                }
            })
            .register();

        loop {
            main_loop.loop_().iterate(Duration::from_secs(60 * 60 * 24));
            if updated.get() {
                updated.set(false);
                client
                    .event_listeners
                    .lock()
                    .unwrap()
                    .retain(|notify| notify.upgrade().inspect(|x| x.notify_one()).is_some());
            }
        }
    }

    fn add_event_listener(&self, notify: &Arc<Notify>) {
        self.event_listeners
            .lock()
            .unwrap()
            .push(Arc::downgrade(notify));
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
    notify: Arc<Notify>,
}

impl<'a> Monitor<'a> {
    pub(super) async fn new(config: &'a Config) -> Result<Self> {
        let client = CLIENT.as_ref().error("Could not get client")?;
        let notify = Arc::new(Notify::new());
        client.add_event_listener(&notify);
        Ok(Self { config, notify })
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
        self.notify.notified().await;
        Ok(())
    }
}

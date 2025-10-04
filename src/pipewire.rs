use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::mem::MaybeUninit;
use std::rc::Rc;
use std::str::FromStr;
use std::sync::{LazyLock, Mutex};
use std::thread;
use std::time::Duration;

pub(crate) use ::pipewire::channel::Sender as PwSender;
use ::pipewire::{
    channel::{Receiver as PwReceiver, channel as pw_channel},
    context::ContextRc,
    device::Device as DeviceProxy,
    keys,
    main_loop::MainLoopRc,
    metadata::Metadata as MetadataProxy,
    node::{Node as NodeProxy, NodeState},
    properties::properties,
    proxy::{Listener, ProxyListener, ProxyT},
    spa::{
        param::ParamType,
        pod::{
            Pod, Value, ValueArray, builder::Builder as PodBuilder, deserialize::PodDeserializer,
        },
        sys::{
            SPA_DIRECTION_INPUT, SPA_DIRECTION_OUTPUT, SPA_PARAM_ROUTE_device,
            SPA_PARAM_ROUTE_direction, SPA_PARAM_ROUTE_index, SPA_PARAM_ROUTE_name,
            SPA_PARAM_ROUTE_props, SPA_PARAM_ROUTE_save, SPA_PROP_channelVolumes, SPA_PROP_mute,
        },
        utils::{SpaTypes, dict::DictRef},
    },
    types::ObjectType,
};
use bitflags::bitflags;
use tokio::sync::mpsc::UnboundedSender;

use crate::{Error, ErrorContext as _, Result};

make_log_macro!(debug, "pipewire");

pub(crate) static CLIENT: LazyLock<Result<Client>> = LazyLock::new(Client::new);

const NORMAL: f32 = 100.0;
const DEFAULT_SINK_KEY: &str = "default.audio.sink";
const DEFAULT_SOURCE_KEY: &str = "default.audio.source";

#[derive(Debug, Clone, Copy)]
pub(crate) enum Direction {
    Input,
    Output,
}

impl FromStr for Direction {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.ends_with("/Source") {
            Ok(Self::Input)
        } else if s.ends_with("/Sink") {
            Ok(Self::Output)
        } else {
            Err(Error::new("Invalid media class to determine direction"))
        }
    }
}

impl TryFrom<u32> for Direction {
    type Error = Error;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            SPA_DIRECTION_INPUT => Ok(Self::Input),
            SPA_DIRECTION_OUTPUT => Ok(Self::Output),
            _ => Err(Error::new("Invalid direction value, must be 0 or 1")),
        }
    }
}

#[derive(Debug)]
pub(crate) struct Node {
    proxy_id: u32,
    pub device_id: Option<u32>,
    pub name: String,
    pub nick: Option<String>,
    pub media_class: Option<String>,
    // direction is derived from media_class
    pub direction: Option<Direction>,
    pub media_role: Option<String>,
    //These come from the proxy
    pub running: bool,
    pub muted: Option<bool>,
    pub volume: Option<Vec<f32>>,
    pub description: Option<String>,
    pub form_factor: Option<String>,
}

impl Node {
    fn new(global_id: u32, global_props: &DictRef, proxy_id: u32) -> Self {
        Self {
            proxy_id,
            device_id: global_props
                .get(&keys::DEVICE_ID)
                .and_then(|s| s.parse().ok()),
            name: global_props
                .get(&keys::NODE_NAME)
                .map_or_else(|| format!("node_{global_id}"), |s| s.to_string()),
            nick: global_props.get(&keys::NODE_NICK).map(|s| s.to_string()),
            media_class: global_props.get(&keys::MEDIA_CLASS).map(|s| s.to_string()),
            direction: global_props
                .get(&keys::MEDIA_CLASS)
                .and_then(|s| s.parse().ok()),
            media_role: global_props.get(&keys::MEDIA_ROLE).map(|s| s.to_string()),
            description: global_props
                .get(&keys::NODE_DESCRIPTION)
                .map(|s| s.to_string()),
            form_factor: global_props
                .get(&keys::DEVICE_FORM_FACTOR)
                .map(|s| s.to_string()),
            muted: None,
            volume: None,
            running: false,
        }
    }
}

#[derive(Debug, PartialEq, PartialOrd, Eq, Ord)]
pub(crate) struct Link {
    pub link_output_node: u32,
    pub link_input_node: u32,
}

impl Link {
    fn new(global_props: &DictRef) -> Option<Self> {
        if let Some(link_output_node) = global_props
            .get(&keys::LINK_OUTPUT_NODE)
            .and_then(|s| s.parse().ok())
            && let Some(link_input_node) = global_props
                .get(&keys::LINK_INPUT_NODE)
                .and_then(|s| s.parse().ok())
        {
            Some(Self {
                link_output_node,
                link_input_node,
            })
        } else {
            None
        }
    }
}

#[derive(Debug)]
pub(crate) struct Route {
    index: i32,
    device: i32,
    pub name: String,
}

#[derive(Debug, Default)]
pub(crate) struct DirectedRoutes {
    proxy_id: u32,
    //These come from the proxy
    input: Option<Route>,
    output: Option<Route>,
}

impl DirectedRoutes {
    fn new(proxy_id: u32) -> Self {
        Self {
            proxy_id,
            input: None,
            output: None,
        }
    }

    pub fn get_route(&self, direction: Direction) -> Option<&Route> {
        match direction {
            Direction::Input => self.input.as_ref(),
            Direction::Output => self.output.as_ref(),
        }
    }

    fn get_mut_route(&mut self, direction: Direction) -> &mut Option<Route> {
        match direction {
            Direction::Input => &mut self.input,
            Direction::Output => &mut self.output,
        }
    }
}

#[derive(Default)]
pub(crate) struct DefaultMetadata {
    pub sink: Option<String>,
    pub source: Option<String>,
}

#[derive(Default)]
pub(crate) struct Data {
    pub nodes: HashMap<u32, Node>,
    pub links: HashMap<u32, Link>,
    pub default_metadata: DefaultMetadata,
    pub directed_routes: HashMap<u32, DirectedRoutes>,
    ports: HashSet<u32>,
}

struct Proxies<T: ProxyT + 'static> {
    proxies_t: HashMap<u32, T>,
    listeners: HashMap<u32, Vec<Box<dyn Listener>>>,
}

impl<T: ProxyT + 'static> Proxies<T> {
    fn new() -> Self {
        Self {
            proxies_t: HashMap::new(),
            listeners: HashMap::new(),
        }
    }

    fn add_proxy(
        &mut self,
        proxy: T,
        listener: impl Listener + 'static,
        proxies: &Rc<RefCell<Self>>,
    ) -> u32 {
        let listener_spe = Box::new(listener);

        let proxy_upcast = proxy.upcast_ref();
        let proxy_id = proxy_upcast.id();

        let proxies_weak = Rc::downgrade(proxies);

        let listener = proxy_upcast
            .add_listener_local()
            .removed(move || {
                if let Some(proxies) = proxies_weak.upgrade() {
                    proxies.borrow_mut().remove(proxy_id);
                }
            })
            .register();

        self.add_proxy_t(proxy_id, proxy, listener_spe);
        self.add_proxy_listener(proxy_id, listener);

        proxy_id
    }

    fn add_proxy_t(&mut self, proxy_id: u32, device_proxy: T, listener: Box<dyn Listener>) {
        self.proxies_t.insert(proxy_id, device_proxy);
        self.listeners.entry(proxy_id).or_default().push(listener);
    }

    fn add_proxy_listener(&mut self, proxy_id: u32, listener: ProxyListener) {
        self.listeners
            .entry(proxy_id)
            .or_default()
            .push(Box::new(listener));
    }

    fn remove(&mut self, proxy_id: u32) {
        self.proxies_t.remove(&proxy_id);
        self.listeners.remove(&proxy_id);
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy, Default)]
    pub(crate) struct EventKind: u16 {
        const DEFAULT_META_DATA_UPDATED = 1 <<  0;
        const DEVICE_ADDED              = 1 <<  1;
        const DEVICE_PARAM_UPDATE       = 1 <<  2;
        const DEVICE_REMOVED            = 1 <<  3;
        const LINK_ADDED                = 1 <<  4;
        const LINK_REMOVED              = 1 <<  5;
        const NODE_ADDED                = 1 <<  6;
        const NODE_PARAM_UPDATE         = 1 <<  7;
        const NODE_REMOVED              = 1 <<  8;
        const NODE_STATE_UPDATE         = 1 <<  9;
        const PORT_ADDED                = 1 << 10;
        const PORT_REMOVED              = 1 << 11;
    }
}

#[derive(Clone, Debug)]
pub(crate) enum CommandKind {
    Mute(u32, bool),
    SetVolume(u32, Vec<f32>),
}

impl CommandKind {
    fn execute(
        self,
        client: &Client,
        node_proxies: Rc<RefCell<Proxies<NodeProxy>>>,
        device_proxies: Rc<RefCell<Proxies<DeviceProxy>>>,
    ) {
        debug!("Executing command: {:?}", self);
        use CommandKind::*;
        let id = match self {
            SetVolume(id, _) | Mute(id, _) => id,
        };
        let client_data = client.data.lock().unwrap();
        if let Some(node) = client_data.nodes.get(&id) {
            if let Some(node_proxy) = node_proxies.borrow_mut().proxies_t.get(&node.proxy_id) {
                let mut pod_data = Vec::new();
                let mut pod_builder = PodBuilder::new(&mut pod_data);

                let mut object_param_props_frame = MaybeUninit::uninit();

                // Safety: Frames must be popped in the reverse order they were pushed.
                // push_object initializes the frame.
                // object_param_props_frame is frame 1
                unsafe {
                    pod_builder.push_object(
                        &mut object_param_props_frame,
                        SpaTypes::ObjectParamProps.as_raw(),
                        ParamType::Props.as_raw(),
                    )
                }
                .expect("Could not push object");

                match &self {
                    SetVolume(_, volume) => {
                        pod_builder
                            .add_prop(SPA_PROP_channelVolumes, 0)
                            .expect("Could not add prop");

                        let mut array_frame = MaybeUninit::uninit();

                        // Safety: push_array initializes the frame.
                        // array_frame is frame 2
                        unsafe { pod_builder.push_array(&mut array_frame) }
                            .expect("Could not push object");

                        for vol in volume {
                            let vol = vol / NORMAL;
                            pod_builder
                                .add_float(vol * vol * vol)
                                .expect("Could not add bool");
                        }

                        // Safety: array_frame is popped here, which is frame 2
                        unsafe {
                            PodBuilder::pop(&mut pod_builder, array_frame.assume_init_mut());
                        }
                    }
                    Mute(_, mute) => {
                        pod_builder
                            .add_prop(SPA_PROP_mute, 0)
                            .expect("Could not add prop");
                        pod_builder.add_bool(*mute).expect("Could not add bool");
                    }
                }

                // Safety: object_param_props_frame is popped here, which is frame 1
                unsafe {
                    PodBuilder::pop(&mut pod_builder, object_param_props_frame.assume_init_mut());
                }

                debug!(
                    "Setting Node Props param: {:?}",
                    PodDeserializer::deserialize_from::<Value>(&pod_data)
                );
                let pod = Pod::from_bytes(&pod_data).expect("Unable to construct pod");
                node_proxy.set_param(ParamType::Props, 0, pod);
            }

            if let Some(device_id) = node.device_id
                && let Some(direction) = node.direction
                && let Some(directed_routes) = client_data.directed_routes.get(&device_id)
                && let Some(route) = directed_routes.get_route(direction)
                && let Some(device_proxy) = device_proxies
                    .borrow_mut()
                    .proxies_t
                    .get(&directed_routes.proxy_id)
            {
                let mut pod_data = Vec::new();
                let mut pod_builder = PodBuilder::new(&mut pod_data);

                let mut object_param_route_frame = MaybeUninit::uninit();

                // Safety: Frames must be popped in the reverse order they were pushed.
                // push_object initializes the frame.
                // object_param_route_frame is frame 1
                unsafe {
                    pod_builder.push_object(
                        &mut object_param_route_frame,
                        SpaTypes::ObjectParamRoute.as_raw(),
                        ParamType::Route.as_raw(),
                    )
                }
                .expect("Could not push object");

                pod_builder
                    .add_prop(SPA_PARAM_ROUTE_index, 0)
                    .expect("Could not add prop");

                pod_builder.add_int(route.index).expect("Could not add int");

                pod_builder
                    .add_prop(SPA_PARAM_ROUTE_device, 0)
                    .expect("Could not add prop");

                pod_builder
                    .add_int(route.device)
                    .expect("Could not add int");

                pod_builder
                    .add_prop(SPA_PARAM_ROUTE_props, 0)
                    .expect("Could not add prop");

                let mut object_param_props_frame = MaybeUninit::uninit();

                // Safety: object_param_props_frame is frame 2
                unsafe {
                    pod_builder.push_object(
                        &mut object_param_props_frame,
                        SpaTypes::ObjectParamProps.as_raw(),
                        ParamType::Route.as_raw(),
                    )
                }
                .expect("Could not push object");

                match &self {
                    SetVolume(_, volume) => {
                        pod_builder
                            .add_prop(SPA_PROP_channelVolumes, 0)
                            .expect("Could not add prop");

                        let mut array_frame = MaybeUninit::uninit();

                        // Safety: push_array initializes the frame.
                        // array_frame is frame 3
                        unsafe { pod_builder.push_array(&mut array_frame) }
                            .expect("Could not push object");

                        for vol in volume {
                            let vol = vol / NORMAL;
                            pod_builder
                                .add_float(vol * vol * vol)
                                .expect("Could not add bool");
                        }

                        // Safety: array_frame is popped here, which is frame 3
                        unsafe {
                            PodBuilder::pop(&mut pod_builder, array_frame.assume_init_mut());
                        }
                    }
                    Mute(_, mute) => {
                        pod_builder
                            .add_prop(SPA_PROP_mute, 0)
                            .expect("Could not add prop");
                        pod_builder.add_bool(*mute).expect("Could not add bool");
                    }
                }

                // Safety: object_param_props_frame is popped here, which is frame 2
                unsafe {
                    PodBuilder::pop(&mut pod_builder, object_param_props_frame.assume_init_mut());
                }

                pod_builder
                    .add_prop(SPA_PARAM_ROUTE_save, 0)
                    .expect("Could not add prop");
                pod_builder.add_bool(true).expect("Could not add bool");

                // Safety: object_param_route_frame is popped here, which is frame 1
                unsafe {
                    PodBuilder::pop(&mut pod_builder, object_param_route_frame.assume_init_mut());
                }

                debug!(
                    "Setting Device Route param: {:?}",
                    PodDeserializer::deserialize_from::<Value>(&pod_data)
                );
                let pod = Pod::from_bytes(&pod_data).expect("Unable to construct pod");
                device_proxy.set_param(ParamType::Route, 0, pod);
            }
        }
    }
}

pub(crate) struct Client {
    event_senders: Mutex<Vec<UnboundedSender<EventKind>>>,
    command_sender: PwSender<CommandKind>,
    pub data: Mutex<Data>,
}

impl Client {
    fn new() -> Result<Client> {
        let (tx, rx) = pw_channel();

        thread::Builder::new()
            .name("i3status_pipewire".to_string())
            .spawn(|| Client::main_loop_thread(rx))
            .error("failed to spawn a thread")?;

        Ok(Self {
            event_senders: Mutex::new(Vec::new()),
            command_sender: tx,
            data: Mutex::new(Data::default()),
        })
    }

    fn main_loop_thread(command_receiver: PwReceiver<CommandKind>) {
        let client = CLIENT.as_ref().error("Could not get client").unwrap();

        let proplist = properties! {*keys::APP_NAME => env!("CARGO_PKG_NAME")};

        let main_loop = MainLoopRc::new(None).expect("Failed to create main loop");

        let context = ContextRc::new(&main_loop, Some(proplist)).expect("Failed to create context");
        let core = context.connect_rc(None).expect("Failed to connect");
        let registry = core.get_registry_rc().expect("Failed to get registry");
        let registry_weak = registry.downgrade();

        let update = Rc::new(RefCell::new(EventKind::empty()));
        let update_copy = update.clone();
        let update_copy2 = update.clone();

        // Proxies and their listeners need to stay alive so store them here
        let node_proxies = Rc::new(RefCell::new(Proxies::<NodeProxy>::new()));
        let node_proxies_weak = Rc::downgrade(&node_proxies);
        let device_proxies = Rc::new(RefCell::new(Proxies::<DeviceProxy>::new()));
        let device_proxies_weak = Rc::downgrade(&device_proxies);
        let metadata_proxies = Rc::new(RefCell::new(Proxies::<MetadataProxy>::new()));

        let _receiver = command_receiver.attach(main_loop.loop_(), move |command: CommandKind| {
            if let Some(node_proxies) = node_proxies_weak.upgrade()
                && let Some(device_proxies) = device_proxies_weak.upgrade()
            {
                command.execute(client, node_proxies.clone(), device_proxies.clone());
            }
        });

        // Register a callback to the `global` event on the registry, which notifies of any new global objects
        // appearing on the remote.
        // The callback will only get called as long as we keep the returned listener alive.
        let _registry_listener = registry
            .add_listener_local()
            .global(move |global| {
                let Some(registry) = registry_weak.upgrade() else {
                    return;
                };
                let global_id = global.id;
                let Some(global_props) = global.props else {
                    return;
                };
                match &global.type_ {
                    ObjectType::Node => {
                        let node_proxy: NodeProxy =
                            registry.bind(global).expect("Could not bind node");
                        node_proxy.subscribe_params(&[ParamType::Props]);
                        let update_copy2 = update_copy.clone();
                        let update_copy3 = update_copy.clone();
                        let node_listener = node_proxy
                            .add_listener_local()
                            .info(move |info| {
                                let running = matches!(info.state(), NodeState::Running);
                                client
                                    .data
                                    .lock()
                                    .unwrap()
                                    .nodes
                                    .entry(global_id)
                                    .and_modify(|node| {
                                        if node.running != running {
                                            node.running = running;
                                            update_copy2.replace_with(|v| {
                                                *v | EventKind::NODE_STATE_UPDATE
                                            });
                                        }
                                    });
                            })
                            .param(move |_seq, _id, _index, _next, param| {
                                let Some(param) = param else {
                                    return;
                                };
                                let Ok((_, Value::Object(object))) =
                                    PodDeserializer::deserialize_from::<Value>(param.as_bytes())
                                else {
                                    return;
                                };
                                client
                                    .data
                                    .lock()
                                    .unwrap()
                                    .nodes
                                    .entry(global_id)
                                    .and_modify(|node| {
                                        for property in object.properties {
                                            if property.key == SPA_PROP_mute {
                                                let Value::Bool(muted) = property.value else {
                                                    return;
                                                };
                                                let muted = Some(muted);
                                                if node.muted != muted {
                                                    node.muted = muted;
                                                    update_copy3.replace_with(|v| {
                                                        *v | EventKind::NODE_PARAM_UPDATE
                                                    });
                                                }
                                            } else if property.key == SPA_PROP_channelVolumes {
                                                let Value::ValueArray(ValueArray::Float(volumes)) =
                                                    property.value
                                                else {
                                                    return;
                                                };

                                                let volume = Some(
                                                    volumes
                                                        .iter()
                                                        .map(|vol| vol.cbrt() * NORMAL)
                                                        .collect(),
                                                );
                                                if node.volume != volume {
                                                    node.volume = volume;
                                                    update_copy3.replace_with(|v| {
                                                        *v | EventKind::NODE_PARAM_UPDATE
                                                    });
                                                }
                                            }
                                        }
                                    });
                            })
                            .register();

                        let proxy_id = node_proxies.borrow_mut().add_proxy(
                            node_proxy,
                            node_listener,
                            &node_proxies,
                        );

                        client
                            .data
                            .lock()
                            .unwrap()
                            .nodes
                            .insert(global_id, Node::new(global_id, global_props, proxy_id));
                        update_copy.replace_with(|v| *v | EventKind::NODE_ADDED);
                    }
                    ObjectType::Link => {
                        let Some(link) = Link::new(global_props) else {
                            return;
                        };
                        client.data.lock().unwrap().links.insert(global_id, link);
                        update_copy.replace_with(|v| *v | EventKind::LINK_ADDED);
                    }
                    ObjectType::Port => {
                        client.data.lock().unwrap().ports.insert(global_id);
                        update_copy.replace_with(|v| *v | EventKind::PORT_ADDED);
                    }
                    ObjectType::Device => {
                        let device_proxy: DeviceProxy =
                            registry.bind(global).expect("Could not bind device");
                        device_proxy.subscribe_params(&[ParamType::Route]);
                        let update_copy2 = update_copy.clone();
                        let device_listener = device_proxy
                            .add_listener_local()
                            .param(move |_seq, _id, _index, _next, param| {
                                let Some(param) = param else {
                                    return;
                                };
                                let Ok((_, Value::Object(object))) =
                                    PodDeserializer::deserialize_from::<Value>(param.as_bytes())
                                else {
                                    return;
                                };
                                let mut route_index = None;
                                let mut route_direction = None;
                                let mut route_device = None;
                                let mut route_name = None;
                                for property in &object.properties {
                                    if property.key == SPA_PARAM_ROUTE_index {
                                        let Value::Int(route_index_v) = property.value else {
                                            return;
                                        };
                                        route_index = Some(route_index_v);
                                    } else if property.key == SPA_PARAM_ROUTE_direction {
                                        let Value::Id(route_direction_v) = property.value else {
                                            return;
                                        };
                                        route_direction = route_direction_v.0.try_into().ok();
                                    } else if property.key == SPA_PARAM_ROUTE_device {
                                        let Value::Int(route_device_v) = property.value else {
                                            return;
                                        };
                                        route_device = Some(route_device_v);
                                    } else if property.key == SPA_PARAM_ROUTE_name {
                                        let Value::String(route_name_v) = property.value.to_owned()
                                        else {
                                            return;
                                        };
                                        route_name = Some(route_name_v);
                                    }
                                }

                                if let Some(route_index) = route_index
                                    && let Some(route_direction) = route_direction
                                    && let Some(route_device) = route_device
                                    && let Some(route_name) = route_name
                                {
                                    client
                                        .data
                                        .lock()
                                        .unwrap()
                                        .directed_routes
                                        .entry(global_id)
                                        .and_modify(|directed_routes| {
                                            let route =
                                                directed_routes.get_mut_route(route_direction);
                                            if let Some(route) = route {
                                                if route.index != route_index
                                                    || route.device != route_device
                                                    || route.name != route_name
                                                {
                                                    route.index = route_index;
                                                    route.device = route_device;
                                                    route.name = route_name;
                                                    update_copy2.replace_with(|v| {
                                                        *v | EventKind::DEVICE_PARAM_UPDATE
                                                    });
                                                }
                                            } else {
                                                route.replace(Route {
                                                    index: route_index,
                                                    device: route_device,
                                                    name: route_name,
                                                });
                                                update_copy2.replace_with(|v| {
                                                    *v | EventKind::DEVICE_PARAM_UPDATE
                                                });
                                            }
                                        });
                                }
                            })
                            .register();

                        let proxy_id = device_proxies.borrow_mut().add_proxy(
                            device_proxy,
                            device_listener,
                            &device_proxies,
                        );

                        client
                            .data
                            .lock()
                            .unwrap()
                            .directed_routes
                            .insert(global_id, DirectedRoutes::new(proxy_id));

                        update_copy.replace_with(|v| *v | EventKind::DEVICE_ADDED);
                    }
                    ObjectType::Metadata => {
                        // There are many kinds of metadata, but we are only interested in the default metadata
                        if global_props.get("metadata.name") != Some("default") {
                            return;
                        }
                        let metadata_proxy: MetadataProxy =
                            registry.bind(global).expect("Could not bind device");
                        let update_copy2 = update_copy.clone();
                        let metadata_listener = metadata_proxy
                            .add_listener_local()
                            .property(move |_subject, key, type_, value| {
                                if type_ != Some("Spa:String:JSON")
                                    || (key != Some(DEFAULT_SINK_KEY)
                                        && key != Some(DEFAULT_SOURCE_KEY))
                                {
                                    return -1;
                                }

                                let Some(value) = value else {
                                    return -1;
                                };

                                let value: serde_json::Value =
                                    serde_json::from_str(value).unwrap_or_default();
                                let name = value
                                    .get("name")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string());
                                if key == Some(DEFAULT_SINK_KEY) {
                                    let default_medata_sink =
                                        &mut client.data.lock().unwrap().default_metadata.sink;
                                    if *default_medata_sink != name {
                                        *default_medata_sink = name;
                                        update_copy2.replace_with(|v| {
                                            *v | EventKind::DEFAULT_META_DATA_UPDATED
                                        });
                                    }
                                } else {
                                    let default_medata_source =
                                        &mut client.data.lock().unwrap().default_metadata.source;
                                    if *default_medata_source != name {
                                        *default_medata_source = name;
                                        update_copy2.replace_with(|v| {
                                            *v | EventKind::DEFAULT_META_DATA_UPDATED
                                        });
                                    }
                                }

                                0
                            })
                            .register();

                        metadata_proxies.borrow_mut().add_proxy(
                            metadata_proxy,
                            metadata_listener,
                            &metadata_proxies,
                        );
                    }
                    _ => (),
                }
            })
            .global_remove(move |uid| {
                let mut client_data = client.data.lock().unwrap();
                if client_data.nodes.remove(&uid).is_some() {
                    update_copy2.replace_with(|v| *v | EventKind::NODE_REMOVED);
                } else if client_data.links.remove(&uid).is_some() {
                    update_copy2.replace_with(|v| *v | EventKind::LINK_REMOVED);
                } else if client_data.ports.remove(&uid) {
                    update_copy2.replace_with(|v| *v | EventKind::PORT_REMOVED);
                } else if client_data.directed_routes.remove(&uid).is_some() {
                    update_copy2.replace_with(|v| *v | EventKind::DEVICE_REMOVED);
                }
            })
            .register();

        loop {
            main_loop.loop_().iterate(Duration::from_hours(24));
            let event = update.take();
            if !event.is_empty() {
                client.send_update_event(event);
            }
        }
    }

    pub fn add_event_listener(&self, tx: UnboundedSender<EventKind>) {
        self.event_senders.lock().unwrap().push(tx);
    }

    pub fn add_command_listener(&self) -> PwSender<CommandKind> {
        self.command_sender.clone()
    }

    pub fn send_update_event(&self, event: EventKind) {
        debug!("send_update_event {:?}", event);
        self.event_senders
            .lock()
            .unwrap()
            .retain(|tx| tx.send(event).is_ok());
    }
}

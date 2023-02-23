use libpulse_binding::callbacks::ListResult;
use libpulse_binding::context::{
    introspect::ServerInfo, introspect::SinkInfo, introspect::SourceInfo, subscribe::Facility,
    subscribe::InterestMaskSet, subscribe::Operation as SubscribeOperation, Context, FlagSet,
    State as PulseState,
};
use libpulse_binding::mainloop::standard::{IterateResult, Mainloop};
use libpulse_binding::proplist::{properties, Proplist};
use libpulse_binding::volume::{ChannelVolumes, Volume};

use crossbeam_channel::{unbounded, Sender};

use std::cmp::{max, min};
use std::convert::{TryFrom, TryInto};
use std::sync::Mutex;
use std::thread;

use super::super::prelude::*;
use super::{DeviceKind, SoundDevice};

static CLIENT: Lazy<Result<Client>> = Lazy::new(Client::new);
static EVENT_LISTENER: Mutex<Vec<tokio::sync::mpsc::Sender<()>>> = Mutex::new(Vec::new());
static DEVICES: Lazy<Mutex<HashMap<(DeviceKind, String), VolInfo>>> = Lazy::new(default);

// Default device names
pub(super) static DEFAULT_SOURCE: Mutex<Cow<'static, str>> =
    Mutex::new(Cow::Borrowed("@DEFAULT_SOURCE@"));
pub(super) static DEFAULT_SINK: Mutex<Cow<'static, str>> =
    Mutex::new(Cow::Borrowed("@DEFAULT_SINK@"));

pub(super) struct Device {
    name: Option<String>,
    description: Option<String>,
    active_port: Option<String>,
    form_factor: Option<String>,
    device_kind: DeviceKind,
    volume: Option<ChannelVolumes>,
    volume_avg: u32,
    muted: bool,
    updates: tokio::sync::mpsc::Receiver<()>,
}

struct Connection {
    mainloop: Mainloop,
    context: Context,
}

struct Client {
    sender: Sender<ClientRequest>,
}

#[derive(Debug)]
struct VolInfo {
    volume: ChannelVolumes,
    mute: bool,
    name: String,
    description: Option<String>,
    active_port: Option<String>,
    form_factor: Option<String>,
}

impl TryFrom<&SourceInfo<'_>> for VolInfo {
    type Error = ();

    fn try_from(source_info: &SourceInfo) -> std::result::Result<Self, Self::Error> {
        match source_info.name.as_ref() {
            None => Err(()),
            Some(name) => Ok(VolInfo {
                volume: source_info.volume,
                mute: source_info.mute,
                name: name.to_string(),
                description: source_info.description.as_ref().map(|d| d.to_string()),
                active_port: source_info
                    .active_port
                    .as_ref()
                    .and_then(|a| a.name.as_ref().map(|n| n.to_string())),
                form_factor: source_info.proplist.get_str(properties::DEVICE_FORM_FACTOR),
            }),
        }
    }
}

impl TryFrom<&SinkInfo<'_>> for VolInfo {
    type Error = ();

    fn try_from(sink_info: &SinkInfo) -> std::result::Result<Self, Self::Error> {
        match sink_info.name.as_ref() {
            None => Err(()),
            Some(name) => Ok(VolInfo {
                volume: sink_info.volume,
                mute: sink_info.mute,
                name: name.to_string(),
                description: sink_info.description.as_ref().map(|d| d.to_string()),
                active_port: sink_info
                    .active_port
                    .as_ref()
                    .and_then(|a| a.name.as_ref().map(|n| n.to_string())),
                form_factor: sink_info.proplist.get_str(properties::DEVICE_FORM_FACTOR),
            }),
        }
    }
}

#[derive(Debug)]
enum ClientRequest {
    GetDefaultDevice,
    GetInfoByIndex(DeviceKind, u32),
    GetInfoByName(DeviceKind, String),
    SetVolumeByName(DeviceKind, String, ChannelVolumes),
    SetMuteByName(DeviceKind, String, bool),
}

impl Connection {
    fn new() -> Result<Self> {
        let mut proplist = Proplist::new().unwrap();
        proplist
            .set_str(properties::APPLICATION_NAME, env!("CARGO_PKG_NAME"))
            .map_err(|_| Error::new("Could not set pulseaudio APPLICATION_NAME property"))?;

        let mainloop = Mainloop::new().error("Failed to create pulseaudio mainloop")?;

        let mut context = Context::new_with_proplist(
            &mainloop,
            concat!(env!("CARGO_PKG_NAME"), "_context"),
            &proplist,
        )
        .error("Failed to create new pulseaudio context")?;

        context
            .connect(None, FlagSet::NOFLAGS, None)
            .error("Failed to connect to pulseaudio context")?;

        let mut connection = Connection { mainloop, context };

        // Wait for context to be ready
        loop {
            connection.iterate(false)?;
            match connection.context.get_state() {
                PulseState::Ready => {
                    break;
                }
                PulseState::Failed | PulseState::Terminated => {
                    return Err(Error::new("pulseaudio context state failed/terminated"));
                }
                _ => {}
            }
        }

        Ok(connection)
    }

    fn iterate(&mut self, blocking: bool) -> Result<()> {
        match self.mainloop.iterate(blocking) {
            IterateResult::Quit(_) | IterateResult::Err(_) => {
                Err(Error::new("failed to iterate pulseaudio state"))
            }
            IterateResult::Success(_) => Ok(()),
        }
    }
}

impl Client {
    fn new() -> Result<Client> {
        let (send_req, recv_req) = unbounded();
        let (send_result, recv_result) = unbounded();
        let send_result2 = send_result.clone();
        let new_connection = |sender: Sender<Result<()>>| -> Connection {
            let conn = Connection::new();
            match conn {
                Ok(conn) => {
                    sender.send(Ok(())).unwrap();
                    conn
                }
                Err(err) => {
                    sender.send(Err(err)).unwrap();
                    panic!("failed to create pulseaudio connection");
                }
            }
        };

        // requests
        thread::Builder::new()
            .name("sound_pulseaudio_req".into())
            .spawn(move || {
                let mut connection = new_connection(send_result);

                loop {
                    // make sure mainloop dispatched everything
                    loop {
                        connection.iterate(false).unwrap();
                        if connection.context.get_state() == PulseState::Ready {
                            break;
                        }
                    }

                    match recv_req.recv() {
                        Err(_) => {}
                        Ok(req) => {
                            use ClientRequest::*;
                            let mut introspector = connection.context.introspect();

                            match req {
                                GetDefaultDevice => {
                                    introspector.get_server_info(Client::server_info_callback);
                                }
                                GetInfoByIndex(DeviceKind::Sink, index) => {
                                    introspector
                                        .get_sink_info_by_index(index, Client::sink_info_callback);
                                }
                                GetInfoByIndex(DeviceKind::Source, index) => {
                                    introspector.get_source_info_by_index(
                                        index,
                                        Client::source_info_callback,
                                    );
                                }
                                GetInfoByName(DeviceKind::Sink, name) => {
                                    introspector
                                        .get_sink_info_by_name(&name, Client::sink_info_callback);
                                }
                                GetInfoByName(DeviceKind::Source, name) => {
                                    introspector.get_source_info_by_name(
                                        &name,
                                        Client::source_info_callback,
                                    );
                                }
                                SetVolumeByName(DeviceKind::Sink, name, volumes) => {
                                    introspector.set_sink_volume_by_name(&name, &volumes, None);
                                }
                                SetVolumeByName(DeviceKind::Source, name, volumes) => {
                                    introspector.set_source_volume_by_name(&name, &volumes, None);
                                }
                                SetMuteByName(DeviceKind::Sink, name, mute) => {
                                    introspector.set_sink_mute_by_name(&name, mute, None);
                                }
                                SetMuteByName(DeviceKind::Source, name, mute) => {
                                    introspector.set_source_mute_by_name(&name, mute, None);
                                }
                            };

                            // send request and receive response
                            connection.iterate(true).unwrap();
                            connection.iterate(true).unwrap();
                        }
                    }
                }
            })
            .unwrap();
        recv_result
            .recv()
            .error("Failed to receive from pulseaudio thread channel")??;

        // subscribe
        thread::Builder::new()
            .name("sound_pulseaudio_sub".into())
            .spawn(move || {
                let mut connection = new_connection(send_result2);

                // subscribe for events
                connection
                    .context
                    .set_subscribe_callback(Some(Box::new(Client::subscribe_callback)));
                connection.context.subscribe(
                    InterestMaskSet::SERVER | InterestMaskSet::SINK | InterestMaskSet::SOURCE,
                    |_| {},
                );

                connection.mainloop.run().unwrap();
            })
            .unwrap();
        recv_result
            .recv()
            .error("Failed to receive from pulseaudio thread channel")??;

        Ok(Client { sender: send_req })
    }

    fn send(request: ClientRequest) -> Result<()> {
        match CLIENT.as_ref() {
            Ok(client) => {
                client.sender.send(request).unwrap();
                Ok(())
            }
            Err(err) => Err(Error::new(format!(
                "pulseaudio connection failed with error: {err}",
            ))),
        }
    }

    fn server_info_callback(server_info: &ServerInfo) {
        if let Some(default_sink) = server_info.default_sink_name.as_ref() {
            *DEFAULT_SINK.lock().unwrap() = default_sink.to_string().into();
        }

        if let Some(default_source) = server_info.default_source_name.as_ref() {
            *DEFAULT_SOURCE.lock().unwrap() = default_source.to_string().into();
        }

        Client::send_update_event();
    }

    fn get_info_callback<I: TryInto<VolInfo>>(result: ListResult<I>) -> Option<VolInfo> {
        match result {
            ListResult::End | ListResult::Error => None,
            ListResult::Item(info) => info.try_into().ok(),
        }
    }

    fn sink_info_callback(result: ListResult<&SinkInfo>) {
        if let Some(vol_info) = Self::get_info_callback(result) {
            DEVICES
                .lock()
                .unwrap()
                .insert((DeviceKind::Sink, vol_info.name.to_string()), vol_info);

            Client::send_update_event();
        }
    }

    fn source_info_callback(result: ListResult<&SourceInfo>) {
        if let Some(vol_info) = Self::get_info_callback(result) {
            DEVICES
                .lock()
                .unwrap()
                .insert((DeviceKind::Source, vol_info.name.to_string()), vol_info);

            Client::send_update_event();
        }
    }

    fn subscribe_callback(
        facility: Option<Facility>,
        _operation: Option<SubscribeOperation>,
        index: u32,
    ) {
        match facility {
            None => {}
            Some(facility) => match facility {
                Facility::Server => {
                    Client::send(ClientRequest::GetDefaultDevice).ok();
                }
                Facility::Sink => {
                    Client::send(ClientRequest::GetInfoByIndex(DeviceKind::Sink, index)).ok();
                }
                Facility::Source => {
                    Client::send(ClientRequest::GetInfoByIndex(DeviceKind::Source, index)).ok();
                }
                _ => {}
            },
        }
    }

    fn send_update_event() {
        EVENT_LISTENER
            .lock()
            .unwrap()
            .retain(|tx| tx.blocking_send(()).is_ok());
    }
}

impl Device {
    pub(super) fn new(device_kind: DeviceKind, name: Option<String>) -> Result<Self> {
        let (tx, rx) = tokio::sync::mpsc::channel(32);
        EVENT_LISTENER.lock().unwrap().push(tx);

        Client::send(ClientRequest::GetDefaultDevice)?;

        let device = Device {
            name,
            description: None,
            active_port: None,
            form_factor: None,
            device_kind,
            volume: None,
            volume_avg: 0,
            muted: false,
            updates: rx,
        };

        Client::send(ClientRequest::GetInfoByName(device_kind, device.name()))?;

        Ok(device)
    }

    fn name(&self) -> String {
        self.name
            .clone()
            .unwrap_or_else(|| self.device_kind.default_name().into())
    }

    fn volume(&mut self, volume: ChannelVolumes) {
        self.volume = Some(volume);
        self.volume_avg = (volume.avg().0 as f32 / Volume::NORMAL.0 as f32 * 100.0).round() as u32;
    }
}

#[async_trait::async_trait]
impl SoundDevice for Device {
    fn volume(&self) -> u32 {
        self.volume_avg
    }

    fn muted(&self) -> bool {
        self.muted
    }

    fn output_name(&self) -> String {
        self.name()
    }

    fn output_description(&self) -> Option<String> {
        self.description.clone()
    }

    fn active_port(&self) -> Option<&str> {
        self.active_port.as_deref()
    }

    fn form_factor(&self) -> Option<&str> {
        self.active_port.as_deref()
    }

    async fn get_info(&mut self) -> Result<()> {
        let devices = DEVICES.lock().unwrap();

        if let Some(info) = devices.get(&(self.device_kind, self.name())) {
            self.volume(info.volume);
            self.muted = info.mute;
            self.description = info.description.clone();
            self.active_port = info.active_port.clone();
            self.form_factor = info.form_factor.clone();
        }

        Ok(())
    }

    async fn set_volume(&mut self, step: i32, max_vol: Option<u32>) -> Result<()> {
        let mut volume = self.volume.error("Volume unknown")?;

        // apply step to volumes
        let step = (step as f32 * Volume::NORMAL.0 as f32 / 100.0).round() as i32;
        for vol in volume.get_mut().iter_mut() {
            let uncapped_vol = max(0, vol.0 as i32 + step) as u32;
            let capped_vol = if let Some(vol_cap) = max_vol {
                min(
                    uncapped_vol,
                    (vol_cap as f32 * Volume::NORMAL.0 as f32 / 100.0).round() as u32,
                )
            } else {
                uncapped_vol
            };
            vol.0 = min(capped_vol, Volume::MAX.0);
        }

        // update volumes
        self.volume(volume);
        Client::send(ClientRequest::SetVolumeByName(
            self.device_kind,
            self.name(),
            volume,
        ))?;

        Ok(())
    }

    async fn toggle(&mut self) -> Result<()> {
        self.muted = !self.muted;

        Client::send(ClientRequest::SetMuteByName(
            self.device_kind,
            self.name(),
            self.muted,
        ))?;

        Ok(())
    }

    async fn wait_for_update(&mut self) -> Result<()> {
        self.updates
            .recv()
            .await
            .error("Failed to receive new update")
    }
}

use std::cmp::{max, min};
use std::convert::{TryFrom, TryInto};
use std::io;
use std::os::fd::{IntoRawFd, RawFd};
use std::sync::{Arc, Mutex, Weak};
use std::thread;

use libc::c_void;
use libpulse_binding::callbacks::ListResult;
use libpulse_binding::context::{
    introspect::ServerInfo, introspect::SinkInfo, introspect::SourceInfo, subscribe::Facility,
    subscribe::InterestMaskSet, Context, FlagSet, State as PulseState,
};
use libpulse_binding::mainloop::api::MainloopApi;
use libpulse_binding::mainloop::standard::{IterateResult, Mainloop};
use libpulse_binding::proplist::{properties, Proplist};
use libpulse_binding::volume::{ChannelVolumes, Volume};
use tokio::sync::Notify;

use super::super::prelude::*;
use super::{DeviceKind, SoundDevice};

static CLIENT: LazyLock<Result<Client>> = LazyLock::new(Client::new);
static EVENT_LISTENER: Mutex<Vec<Weak<Notify>>> = Mutex::new(Vec::new());
static DEVICES: LazyLock<Mutex<HashMap<(DeviceKind, String), VolInfo>>> = LazyLock::new(default);

// Default device names
pub(super) static DEFAULT_SOURCE: Mutex<Cow<'static, str>> =
    Mutex::new(Cow::Borrowed("@DEFAULT_SOURCE@"));
pub(super) static DEFAULT_SINK: Mutex<Cow<'static, str>> =
    Mutex::new(Cow::Borrowed("@DEFAULT_SINK@"));

impl DeviceKind {
    pub fn default_name(self) -> Cow<'static, str> {
        match self {
            Self::Sink => DEFAULT_SINK.lock().unwrap().clone(),
            Self::Source => DEFAULT_SOURCE.lock().unwrap().clone(),
        }
    }
}

pub(super) struct Device {
    name: Option<String>,
    description: Option<String>,
    active_port: Option<String>,
    form_factor: Option<String>,
    device_kind: DeviceKind,
    volume: Option<ChannelVolumes>,
    volume_avg: u32,
    muted: bool,
    notify: Arc<Notify>,
}

struct Connection {
    mainloop: Mainloop,
    context: Context,
}

struct Client {
    send_req: std::sync::mpsc::Sender<ClientRequest>,
    ml_waker: MainloopWaker,
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

    /// Create connection in a new thread.
    ///
    /// If connection can't be created, Err is returned.
    fn spawn(thread_name: &str, f: impl Fn(Self) -> bool + Send + 'static) -> Result<()> {
        let (tx, rx) = std::sync::mpsc::sync_channel(0);
        thread::Builder::new()
            .name(thread_name.to_owned())
            .spawn(move || match Self::new() {
                Ok(mut conn) => {
                    tx.send(Ok(())).unwrap();
                    while f(conn) {
                        let mut try_i = 0usize;
                        loop {
                            try_i += 1;
                            let delay =
                                Duration::from_millis(if try_i <= 10 { 100 } else { 5_000 });
                            eprintln!("reconnecting to pulseaudio in {delay:?}... (try {try_i})");
                            thread::sleep(delay);
                            if let Ok(c) = Self::new() {
                                eprintln!("reconnected to pulseaudio");
                                conn = c;
                                break;
                            }
                        }
                    }
                }
                Err(err) => {
                    tx.send(Err(err)).unwrap();
                }
            })
            .error("failed to spawn a thread")?;
        rx.recv().error("channel closed")?
    }
}

impl Client {
    fn new() -> Result<Client> {
        let (send_req, recv_req) = std::sync::mpsc::channel();
        let ml_waker = MainloopWaker::new().unwrap();

        Connection::spawn("sound_pulseaudio", move |mut connection| {
            ml_waker.attach(connection.mainloop.get_api());

            let introspector = connection.context.introspect();
            connection
                .context
                .set_subscribe_callback(Some(Box::new(move |facility, _, index| match facility {
                    Some(Facility::Server) => {
                        introspector.get_server_info(Client::server_info_callback);
                    }
                    Some(Facility::Sink) => {
                        introspector.get_sink_info_by_index(index, Client::sink_info_callback);
                    }
                    Some(Facility::Source) => {
                        introspector.get_source_info_by_index(index, Client::source_info_callback);
                    }
                    _ => (),
                })));

            connection.context.subscribe(
                InterestMaskSet::SERVER | InterestMaskSet::SINK | InterestMaskSet::SOURCE,
                |_| (),
            );

            let mut introspector = connection.context.introspect();

            loop {
                loop {
                    connection.iterate(true).unwrap();
                    match connection.context.get_state() {
                        PulseState::Ready => break,
                        PulseState::Failed => return true,
                        _ => (),
                    }
                }

                loop {
                    use std::sync::mpsc::TryRecvError;
                    let req = match recv_req.try_recv() {
                        Ok(x) => x,
                        Err(TryRecvError::Empty) => break,
                        Err(TryRecvError::Disconnected) => return false,
                    };

                    use ClientRequest::*;
                    match req {
                        GetDefaultDevice => {
                            introspector.get_server_info(Client::server_info_callback);
                        }
                        GetInfoByName(DeviceKind::Sink, name) => {
                            introspector.get_sink_info_by_name(&name, Client::sink_info_callback);
                        }
                        GetInfoByName(DeviceKind::Source, name) => {
                            introspector
                                .get_source_info_by_name(&name, Client::source_info_callback);
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
                }
            }
        })?;

        Ok(Client { send_req, ml_waker })
    }

    fn send(request: ClientRequest) -> Result<()> {
        match CLIENT.as_ref() {
            Ok(client) => {
                client.send_req.send(request).unwrap();
                client.ml_waker.wake().unwrap();
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

    fn send_update_event() {
        EVENT_LISTENER
            .lock()
            .unwrap()
            .retain(|notify| notify.upgrade().inspect(|x| x.notify_one()).is_some());
    }
}

impl Device {
    pub(super) fn new(device_kind: DeviceKind, name: Option<String>) -> Result<Self> {
        let notify = Arc::new(Notify::new());
        EVENT_LISTENER.lock().unwrap().push(Arc::downgrade(&notify));

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
            notify,
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

    fn active_port(&self) -> Option<String> {
        self.active_port.clone()
    }

    fn form_factor(&self) -> Option<&str> {
        self.form_factor.as_deref()
    }

    async fn get_info(&mut self) -> Result<()> {
        let devices = DEVICES.lock().unwrap();

        if let Some(info) = devices.get(&(self.device_kind, self.name())) {
            self.volume(info.volume);
            self.muted = info.mute;
            self.description.clone_from(&info.description);
            self.active_port.clone_from(&info.active_port);
            self.form_factor.clone_from(&info.form_factor);
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
        self.notify.notified().await;
        Ok(())
    }
}

/// Thread safe [`Mainloop`] waker.
///
/// Has the same purpose as [`Mainloop::wake`], but can be shared across threads.
#[derive(Debug, Clone, Copy)]
struct MainloopWaker {
    // Note: these fds are never closed, but this is OK because there is only one instance of this struct.
    pipe_tx: RawFd,
    pipe_rx: RawFd,
}

impl MainloopWaker {
    /// Create new waker.
    fn new() -> io::Result<Self> {
        let (pipe_rx, pipe_tx) = nix::unistd::pipe2(nix::fcntl::OFlag::O_CLOEXEC)?;
        Ok(Self {
            pipe_tx: pipe_tx.into_raw_fd(),
            pipe_rx: pipe_rx.into_raw_fd(),
        })
    }

    /// Attach this waker to a [`Mainloop`].
    ///
    /// A waker should be attached to _one_ mainloop.
    fn attach(self, ml: &MainloopApi) {
        extern "C" fn wake_cb(
            _: *const MainloopApi,
            _: *mut libpulse_binding::mainloop::events::io::IoEventInternal,
            fd: RawFd,
            _: libpulse_binding::mainloop::events::io::FlagSet,
            _: *mut c_void,
        ) {
            nix::unistd::read(fd, &mut [0; 32]).unwrap();
        }

        (ml.io_new.unwrap())(
            ml as *const _,
            self.pipe_rx,
            libpulse_binding::mainloop::events::io::FlagSet::INPUT,
            Some(wake_cb),
            std::ptr::null_mut(),
        );
    }

    /// Interrupt blocking [`Mainloop::iterate`].
    fn wake(self) -> io::Result<()> {
        let buf = [0u8];
        let res = unsafe { libc::write(self.pipe_tx, buf.as_ptr().cast(), 1) };
        if res == -1 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

use super::*;
use x11rb_async::{
    connection::{Connection as _, RequestConnection as _},
    protocol::{
        xkb::{
            self, ConnectionExt as _, EventType, MapPart, NameDetail, SelectEventsAux,
            UseExtensionReply, ID,
        },
        xproto::ConnectionExt as _,
        Event,
    },
    rust_connection::RustConnection,
};

const XCB_XKB_MINOR_VERSION: u16 = 0;
const XCB_XKB_MAJOR_VERSION: u16 = 1;

pub(super) struct XkbEvent {
    connection: RustConnection,
}

fn parse_layout(buf: &[u8], index: usize) -> Result<&str> {
    let colon_i = buf.iter().position(|c| *c == b':').unwrap_or(buf.len());
    let layout = buf[..colon_i]
        .split(|&c| c == b'+')
        .skip(1) // layout names start from index 1
        .nth(index)
        .error("Index out of range")?;
    std::str::from_utf8(layout).error("non utf8 layout")
}

async fn get_layout(connection: &RustConnection) -> Result<String> {
    let xkb_state = connection
        .xkb_get_state(ID::USE_CORE_KBD.into())
        .await
        .error("xkb_get_state failed")?
        .reply()
        .await
        .error("xkb_get_state reply failed")?;
    let group: u8 = xkb_state.group.into();

    let symbols_name = connection
        .xkb_get_names(
            ID::USE_CORE_KBD.into(),
            NameDetail::GROUP_NAMES | NameDetail::SYMBOLS,
        )
        .await
        .error("xkb_get_names failed")?
        .reply()
        .await
        .error("xkb_get_names reply failed")?
        .value_list
        .symbols_name
        .error("symbols_name is empty")?;

    let name = connection
        .get_atom_name(symbols_name)
        .await
        .error("get_atom_name failed")?
        .reply()
        .await
        .error("get_atom_name reply failed")?
        .name;
    let layout = parse_layout(&name, group as _)?;

    Ok(layout.to_owned())
}

async fn prefetch_xkb_extension(connection: &RustConnection) -> Result<UseExtensionReply> {
    connection
        .prefetch_extension_information(xkb::X11_EXTENSION_NAME)
        .await
        .error("prefetch_extension_information failed")?;

    let reply = connection
        .xkb_use_extension(XCB_XKB_MAJOR_VERSION, XCB_XKB_MINOR_VERSION)
        .await
        .error("xkb_use_extension failed")?
        .reply()
        .await
        .error("xkb_use_extension reply failed")?;

    Ok(reply)
}

impl XkbEvent {
    pub(super) async fn new() -> Result<Self> {
        let (connection, _, drive) = RustConnection::connect(None)
            .await
            .error("Failed to open XCB connection")?;

        tokio::spawn(drive);
        let reply = prefetch_xkb_extension(&connection)
            .await
            .error("Failed to prefetch xkb extension")?;

        if !reply.supported {
            return Err(Error::new(
                "This program requires the X11 server to support the XKB extension",
            ));
        }

        connection
            .xkb_select_events(
                ID::USE_CORE_KBD.into(),
                EventType::default(),
                EventType::STATE_NOTIFY,
                MapPart::default(),
                MapPart::default(),
                &SelectEventsAux::new(),
            )
            .await
            .error("Failed to select events")?;

        Ok(XkbEvent { connection })
    }
}

#[async_trait]
impl Backend for XkbEvent {
    async fn get_info(&mut self) -> Result<Info> {
        let cur_layout = get_layout(&self.connection)
            .await
            .error("Failed to get current layout")?;
        Ok(Info::from_layout_variant_str(&cur_layout))
    }

    async fn wait_for_change(&mut self) -> Result<()> {
        loop {
            let event = self
                .connection
                .wait_for_event()
                .await
                .error("Failed to read the event")?;

            if let Event::XkbStateNotify(e) = event {
                if e.changed.contains(xkb::StatePart::GROUP_STATE) {
                    return Ok(());
                }
            }
        }
    }
}

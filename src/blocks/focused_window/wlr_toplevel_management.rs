use super::{Backend, Info};
use crate::blocks::prelude::*;

use wayrs_protocols::wlr_foreign_toplevel_management_unstable_v1::*;

use wayrs_client::connection::Connection;
use wayrs_client::global::GlobalsExt;

pub(super) struct WlrToplevelManagement {
    conn: Connection<State>,
    state: State,
}

#[derive(Default)]
struct State {
    error: Option<Error>,
    new_title: Option<String>,
    toplevels: HashMap<ZwlrForeignToplevelHandleV1, Toplevel>,
    active_toplevel: Option<ZwlrForeignToplevelHandleV1>,
}

#[derive(Default)]
struct Toplevel {
    title: Option<String>,
    is_active: bool,
}

impl WlrToplevelManagement {
    pub(super) async fn new() -> Result<Self> {
        let mut conn = Connection::connect().error("failed to connect to wayland")?;
        let globals = conn
            .async_collect_initial_globals()
            .await
            .error("wayland error")?;

        let _: ZwlrForeignToplevelManagerV1 = globals
            .bind_with_cb(&mut conn, 1..=3, toplevel_manager_cb)
            .error("unsupported compositor")?;

        conn.async_flush().await.error("wayland error")?;

        Ok(Self {
            conn,
            state: default(),
        })
    }
}

#[async_trait]
impl Backend for WlrToplevelManagement {
    async fn get_info(&mut self) -> Result<Info> {
        loop {
            self.conn.async_recv_events().await.error("wayland error")?;
            self.conn.dispatch_events(&mut self.state);
            if let Some(err) = self.state.error.take() {
                return Err(err);
            }
            self.conn.async_flush().await.error("wayland error")?;

            if let Some(title) = self.state.new_title.take() {
                return Ok(Info {
                    title,
                    marks: default(),
                });
            }
        }
    }
}

fn toplevel_manager_cb(
    conn: &mut Connection<State>,
    state: &mut State,
    _: ZwlrForeignToplevelManagerV1,
    event: zwlr_foreign_toplevel_manager_v1::Event,
) {
    match event {
        zwlr_foreign_toplevel_manager_v1::Event::Toplevel(toplevel) => {
            state.toplevels.insert(toplevel, default());
            conn.set_callback_for(toplevel, toplevel_cb);
        }
        zwlr_foreign_toplevel_manager_v1::Event::Finished => {
            state.error = Some(Error::new("unexpected 'finished' event"));
            conn.break_dispatch_loop();
        }
        _ => (),
    }
}

fn toplevel_cb(
    conn: &mut Connection<State>,
    state: &mut State,
    wlr_toplevel: ZwlrForeignToplevelHandleV1,
    event: zwlr_foreign_toplevel_handle_v1::Event,
) {
    use zwlr_foreign_toplevel_handle_v1::Event;

    let toplevel = state.toplevels.get_mut(&wlr_toplevel).unwrap();

    match event {
        Event::Title(title) => {
            toplevel.title = Some(String::from_utf8_lossy(title.as_bytes()).into());
        }
        Event::State(state) => {
            toplevel.is_active = state
                .chunks_exact(4)
                .map(|b| u32::from_ne_bytes(b.try_into().unwrap()))
                .any(|s| s == zwlr_foreign_toplevel_handle_v1::State::Activated as u32);
        }
        Event::Closed => {
            if state.active_toplevel == Some(wlr_toplevel) {
                state.active_toplevel = None;
                state.new_title = Some(default());
            }

            wlr_toplevel.destroy(conn);
            state.toplevels.remove(&wlr_toplevel);
        }
        Event::Done => {
            if toplevel.is_active {
                state.active_toplevel = Some(wlr_toplevel);
                state.new_title = Some(toplevel.title.clone().unwrap_or_default());
            } else if state.active_toplevel == Some(wlr_toplevel) {
                state.active_toplevel = None;
                state.new_title = Some(default());
            }
        }
        _ => (),
    }
}

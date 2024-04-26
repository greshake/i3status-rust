use super::{Backend, Info};
use crate::blocks::prelude::*;

use wayrs_protocols::wlr_foreign_toplevel_management_unstable_v1::*;

use wayrs_client::global::GlobalsExt;
use wayrs_client::{Connection, EventCtx};

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
        let (mut conn, globals) = Connection::async_connect_and_collect_globals()
            .await
            .error("failed to connect to wayland")?;

        let _: ZwlrForeignToplevelManagerV1 = globals
            .bind_with_cb(&mut conn, 1..=3, toplevel_manager_cb)
            .error("unsupported compositor")?;

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
            self.conn.async_flush().await.error("wayland error")?;
            self.conn.async_recv_events().await.error("wayland error")?;
            self.conn.dispatch_events(&mut self.state);

            if let Some(err) = self.state.error.take() {
                return Err(err);
            }

            if let Some(title) = self.state.new_title.take() {
                return Ok(Info {
                    title,
                    marks: default(),
                });
            }
        }
    }
}

fn toplevel_manager_cb(ctx: EventCtx<State, ZwlrForeignToplevelManagerV1>) {
    use zwlr_foreign_toplevel_manager_v1::Event;
    match ctx.event {
        Event::Toplevel(toplevel) => {
            ctx.state.toplevels.insert(toplevel, default());
            ctx.conn.set_callback_for(toplevel, toplevel_cb);
        }
        Event::Finished => {
            ctx.state.error = Some(Error::new("unexpected 'finished' event"));
            ctx.conn.break_dispatch_loop();
        }
        _ => (),
    }
}

fn toplevel_cb(ctx: EventCtx<State, ZwlrForeignToplevelHandleV1>) {
    use zwlr_foreign_toplevel_handle_v1::Event;

    let Some(toplevel) = ctx.state.toplevels.get_mut(&ctx.proxy) else {
        return;
    };

    match ctx.event {
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
            if ctx.state.active_toplevel == Some(ctx.proxy) {
                ctx.state.active_toplevel = None;
                ctx.state.new_title = Some(default());
            }

            ctx.proxy.destroy(ctx.conn);
            ctx.state.toplevels.remove(&ctx.proxy);
        }
        Event::Done => {
            if toplevel.is_active {
                ctx.state.active_toplevel = Some(ctx.proxy);
                ctx.state.new_title = Some(toplevel.title.clone().unwrap_or_default());
            } else if ctx.state.active_toplevel == Some(ctx.proxy) {
                ctx.state.active_toplevel = None;
                ctx.state.new_title = Some(default());
            }
        }
        _ => (),
    }
}

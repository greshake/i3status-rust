use super::{Backend, Info};
use crate::blocks::prelude::*;
use swayipc_async::{Connection, Event, EventStream, EventType, WindowChange, WorkspaceChange};

pub(super) struct SwayIpc {
    events: EventStream,
    info: Info,
}

impl SwayIpc {
    pub(super) async fn new() -> Result<Self> {
        Ok(Self {
            events: Connection::new()
                .await
                .error("failed to open connection with swayipc")?
                .subscribe(&[EventType::Window, EventType::Workspace])
                .await
                .error("could not subscribe to window events")?,
            info: default(),
        })
    }
}

#[async_trait]
impl Backend for SwayIpc {
    async fn get_info(&mut self) -> Result<Info> {
        loop {
            let event = self
                .events
                .next()
                .await
                .error("swayipc channel closed")?
                .error("bad event")?;
            match event {
                Event::Window(e) => match e.change {
                    WindowChange::Mark => {
                        self.info.marks = e.container.marks;
                    }
                    WindowChange::Focus => {
                        self.info.title.clear();
                        if let Some(new_title) = &e.container.name {
                            self.info.title.push_str(new_title);
                        }
                        self.info.marks = e.container.marks;
                    }
                    WindowChange::Title => {
                        if e.container.focused {
                            self.info.title.clear();
                            if let Some(new_title) = &e.container.name {
                                self.info.title.push_str(new_title);
                            }
                        } else {
                            continue;
                        }
                    }
                    WindowChange::Close => {
                        self.info.title.clear();
                        self.info.marks.clear();
                    }
                    _ => continue,
                },
                Event::Workspace(e) if e.change == WorkspaceChange::Init => {
                    self.info.title.clear();
                    self.info.marks.clear();
                }
                _ => continue,
            }

            return Ok(self.info.clone());
        }
    }
}

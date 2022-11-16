//! Currently focused window
//!
//! This block displays the title and/or the active marks (when used with `sway`/`i3`) of the currently
//! focused window. Supported WMs are: `sway`, `i3` and `river`. See `driver` option for more info.
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `format` | A string to customise the output of this block. See below for available placeholders. | <code>" $title.str(0,21) &vert;"</code>
//! `driver` | Which driver to use. Available values: `sway_ipc` - for `i3` and `sway`, `ristate` - for `river` (note that [`ristate`](https://gitlab.com/snakedye/ristate) binary must be in the `PATH`), `auto` - try to automatically guess which driver to use. | `"auto"`
//!
//! Placeholder     | Value                                                                 | Type | Unit
//! ----------------|-----------------------------------------------------------------------|------|-----
//! `title`         | Window's title (may be absent)                                        | Text | -
//! `marks`         | Window's marks (present only with sway/i3)                            | Text | -
//! `visible_marks` | Window's marks that do not start with `_` (present only with sway/i3) | Text | -
//!
//! # Example
//!
//! ```toml
//! [[block]]
//! block = "focused_window"
//! [block.format]
//! full = " $title.str(0,15) |"
//! short = " $title.str(0,10) |"
//! ```
//!
//! This example instead of hiding block when the window's title is empty displays "Missing"
//!
//! ```toml
//! [[block]]
//! block = "focused_window"
//! format = " $title.str(0,21) | Missing "

use super::prelude::*;
use swayipc_async::{Connection, Event, EventStream, EventType, WindowChange, WorkspaceChange};

use std::process::Stdio;
use tokio::{
    io::{BufReader, Lines},
    process::{ChildStdout, Command},
};

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(default)]
pub struct Config {
    format: FormatConfig,
    driver: Driver,
}

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(rename_all = "snake_case")]
enum Driver {
    #[default]
    Auto,
    SwayIpc,
    Ristate,
}

pub async fn run(config: Config, mut api: CommonApi) -> Result<()> {
    let mut widget = Widget::new().with_format(config.format.with_default(" $title.str(0,21) |")?);

    let mut backend: Box<dyn Backend> = match config.driver {
        Driver::Auto => match SwayIpc::new().await {
            Ok(swayipc) => Box::new(swayipc),
            Err(_) => Box::new(Ristate::new()?),
        },
        Driver::SwayIpc => Box::new(SwayIpc::new().await?),
        Driver::Ristate => Box::new(Ristate::new()?),
    };

    loop {
        select! {
            _ = api.event() => (),
            info = backend.get_info() => {
                let Info { title, marks } = info?;
                if title.is_empty() {
                    widget.set_values(default());
                } else {
                    widget.set_values(map! {
                        "title" => Value::text(title.clone()),
                        "marks" => Value::text(marks.iter().map(|m| format!("[{m}]")).collect()),
                        "visible_marks" => Value::text(marks.iter().filter(|m| !m.starts_with('_')).map(|m| format!("[{m}]")).collect()),
                    });
                }
                api.set_widget(&widget).await?;
            }
        }
    }
}

#[async_trait]
trait Backend {
    async fn get_info(&mut self) -> Result<Info>;
}

#[derive(Clone, Default)]
struct Info {
    title: String,
    marks: Vec<String>,
}

struct SwayIpc {
    events: EventStream,
    info: Info,
}

impl SwayIpc {
    async fn new() -> Result<Self> {
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

struct Ristate {
    stream: Lines<BufReader<ChildStdout>>,
}

impl Ristate {
    fn new() -> Result<Self> {
        let mut ristate = Command::new("ristate")
            .arg("-w")
            .stdout(Stdio::piped())
            .spawn()
            .error("failed to run ristate")?;
        let stream = BufReader::new(ristate.stdout.take().unwrap()).lines();

        tokio::spawn(async move {
            let _ = ristate.wait().await;
        });

        Ok(Self { stream })
    }
}

#[async_trait]
impl Backend for Ristate {
    async fn get_info(&mut self) -> Result<Info> {
        #[derive(Deserialize, Debug)]
        struct RistateOuput {
            title: String,
        }

        let line = self
            .stream
            .next_line()
            .await
            .error("ristate exited unexpectedly")?
            .error("error reading line from ristate")?;

        let title = serde_json::from_str::<RistateOuput>(&line)
            .error("ristate produced invalid json")?
            .title;

        Ok(Info {
            title,
            marks: default(),
        })
    }
}

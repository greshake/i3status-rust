//! Currently focused window
//!
//! This block displays the title and/or the active marks (when used with `sway`/`i3`) of the currently
//! focused window. Supported WMs are: `sway`, `i3` and `river`. See `driver` option for more info.
//!
//! # Configuration
//!
//! Key | Values | Required | Default
//! ----|--------|----------|--------
//! `format` | A string to customise the output of this block. See below for available placeholders. | No | <code>"$title.rot-str(15)&vert;"</code>
//! `autohide` | Whether to hide the block when no title is available | No | `true`
//! `driver` | Which driver to use. Available values: `sway_ipc` - for `i3` and `sway`, `ristate` - for `river` (note that [`ristate`](https://gitlab.com/snakedye/ristate) binary must be in the `PATH`), `auto` - try to automatically guess which driver to use. | No | `"auto"`
//!
//! Placeholder     | Value                                                                 | Type | Unit
//! ----------------|-----------------------------------------------------------------------|------|-----
//! `title`         | Window's titile (may be absent)                                       | Text | -
//! `marks`         | Window's marks (present only with sway/i3)                            | Text | -
//! `visible_marks` | Window's marks that do not start with `_` (present only with sway/i3) | Text | -
//!
//! # Example
//!
//! ```toml
//! [[block]]
//! block = "focused_window"
//! [block.format]
//! full = "$title.rot-str(15)|"
//! short = "$title.rot-str(10)|"
//! ```

use super::prelude::*;
use swayipc_async::{Connection, Event, EventType, WindowChange, WorkspaceChange};

use std::process::Stdio;
use tokio::{io::BufReader, process::Command};

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields, default)]
struct FocusedWindowConfig {
    format: FormatConfig,
    autohide: bool,
    driver: Driver,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
enum Driver {
    Auto,
    SwayIpc,
    Ristate,
}

impl Default for FocusedWindowConfig {
    fn default() -> Self {
        Self {
            format: Default::default(),
            autohide: true,
            driver: Driver::Auto,
        }
    }
}

pub async fn run(config: toml::Value, mut api: CommonApi) -> Result<()> {
    let config = FocusedWindowConfig::deserialize(config).config_error()?;
    api.set_format(config.format.with_default("$title.rot-str(15)|")?);

    match config.driver {
        Driver::Auto => match Connection::new().await {
            Ok(conn) => with_sway_ipc(conn, config.autohide, &mut api).await,
            Err(_) => with_ristate(config.autohide, &mut api).await,
        },
        Driver::SwayIpc => {
            with_sway_ipc(
                Connection::new()
                    .await
                    .error("failed to open connection with swayipc")?,
                config.autohide,
                &mut api,
            )
            .await
        }
        Driver::Ristate => with_ristate(config.autohide, &mut api).await,
    }
}

async fn with_sway_ipc(conn: Connection, autohide: bool, api: &mut CommonApi) -> Result<()> {
    let mut title = String::new();
    let mut marks = Vec::new();

    let mut events = conn
        .subscribe(&[EventType::Window, EventType::Workspace])
        .await
        .error("could not subscribe to window events")?;

    loop {
        let event = events
            .next()
            .await
            .error("swayipc channel closed")?
            .error("bad event")?;

        let updated = match event {
            Event::Window(e) => match e.change {
                WindowChange::Mark => {
                    marks = e.container.marks;
                    true
                }
                WindowChange::Focus => {
                    title.clear();
                    if let Some(new_title) = &e.container.name {
                        title.push_str(new_title);
                    }
                    marks = e.container.marks;
                    true
                }
                WindowChange::Title => {
                    if e.container.focused {
                        title.clear();
                        if let Some(new_title) = &e.container.name {
                            title.push_str(new_title);
                        }
                        true
                    } else {
                        false
                    }
                }
                WindowChange::Close => {
                    title.clear();
                    marks.clear();
                    true
                }
                _ => false,
            },
            Event::Workspace(e) if e.change == WorkspaceChange::Init => {
                title.clear();
                marks.clear();
                true
            }
            _ => false,
        };

        if updated {
            if !title.is_empty() || !autohide {
                api.set_values(map! {
                    "title" => Value::text(title.clone()),
                    "marks" => Value::text(marks.iter().map(|m| format!("[{m}]")).collect()),
                    "visible_marks" => Value::text(marks.iter().filter(|m| !m.starts_with('_')).map(|m| format!("[{m}]")).collect()),
                });
                api.show();
            } else {
                api.hide();
            }
            api.flush().await?;
        }
    }
}

async fn with_ristate(autohide: bool, api: &mut CommonApi) -> Result<()> {
    #[derive(Deserialize, Debug)]
    struct RistateOuput {
        title: String,
    }

    let mut ristate = Command::new("ristate")
        .arg("-w")
        .stdout(Stdio::piped())
        .spawn()
        .error("failed to run ristate")?;
    let mut stream = BufReader::new(ristate.stdout.take().unwrap()).lines();

    tokio::spawn(async move {
        let _ = ristate.wait().await;
    });

    while let Some(line) = stream
        .next_line()
        .await
        .error("error reading line from ristate")?
    {
        let title = serde_json::from_str::<RistateOuput>(&line)
            .error("ristate produced invalid json")?
            .title;
        if !title.is_empty() || !autohide {
            api.set_values(map! {
                "title" => Value::text(title.clone()),
            });
            api.show();
        } else {
            api.hide();
        }
        api.flush().await?;
    }

    Err(Error::new("ristate exited unexpectedly"))
}

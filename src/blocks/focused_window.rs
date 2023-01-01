//! Currently focused window
//!
//! This block displays the title and/or the active marks (when used with `sway`/`i3`) of the currently
//! focused window. Supported WMs are: `sway`, `i3` and most wlroots-based compositors. See `driver`
//! option for more info.
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `format` | A string to customise the output of this block. See below for available placeholders. | <code>" $title.str(0,21) &vert;"</code>
//! `driver` | Which driver to use. Available values: `sway_ipc` - for `i3` and `sway`, `wlr_toplevel_management` - for Wayland compositors that implement [wlr-foreign-toplevel-management-unstable-v1](https://gitlab.freedesktop.org/wlroots/wlr-protocols/-/blob/master/unstable/wlr-foreign-toplevel-management-unstable-v1.xml), `auto` - try to automatically guess which driver to use. | `"auto"`
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

mod sway_ipc;
mod wlr_toplevel_management;

use sway_ipc::SwayIpc;
use wlr_toplevel_management::WlrToplevelManagement;

use super::prelude::*;

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
    WlrToplevelManagement,
}

pub async fn run(config: Config, mut api: CommonApi) -> Result<()> {
    let mut widget = Widget::new().with_format(config.format.with_default(" $title.str(0,21) |")?);

    let mut backend: Box<dyn Backend> = match config.driver {
        Driver::Auto => match SwayIpc::new().await {
            Ok(swayipc) => Box::new(swayipc),
            Err(_) => Box::new(WlrToplevelManagement::new().await?),
        },
        Driver::SwayIpc => Box::new(SwayIpc::new().await?),
        Driver::WlrToplevelManagement => Box::new(WlrToplevelManagement::new().await?),
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

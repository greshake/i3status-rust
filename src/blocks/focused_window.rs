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
//! `format` | A string to customise the output of this block. See below for available placeholders. | <code>\" $title.str(max_w:21) \|\"</code>
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
//! full = " $title.str(max_w:15) |"
//! short = " $title.str(max_w:10) |"
//! ```
//!
//! This example instead of hiding block when the window's title is empty displays "Missing"
//!
//! ```toml
//! [[block]]
//! block = "focused_window"
//! format = " $title.str(0,21) | Missing "
//! ```

mod sway_ipc;
mod wlr_toplevel_management;

use sway_ipc::SwayIpc;
use wlr_toplevel_management::WlrToplevelManagement;

use super::prelude::*;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub format: FormatConfig,
    pub driver: Driver,
}

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(rename_all = "snake_case")]
pub enum Driver {
    #[default]
    Auto,
    SwayIpc,
    WlrToplevelManagement,
}

pub async fn run(config: &Config, api: &CommonApi) -> Result<()> {
    let format = config.format.with_default(" $title.str(max_w:21) |")?;

    let mut backend: Box<dyn Backend> = match config.driver {
        Driver::Auto => match SwayIpc::new().await {
            Ok(swayipc) => Box::new(swayipc),
            Err(_) => Box::new(WlrToplevelManagement::new().await?),
        },
        Driver::SwayIpc => Box::new(SwayIpc::new().await?),
        Driver::WlrToplevelManagement => Box::new(WlrToplevelManagement::new().await?),
    };

    loop {
        let Info { title, marks } = backend.get_info().await?;

        let mut widget = Widget::new().with_format(format.clone());

        if !title.is_empty() {
            let join_marks = |mut s: String, m: &String| {
                let _ = write!(s, "[{m}]"); // writing to String never fails
                s
            };

            let marks_str = marks.iter().fold(String::new(), join_marks);
            let visible_marks_str = marks
                .iter()
                .filter(|m| !m.starts_with('_'))
                .fold(String::new(), join_marks);

            widget.set_values(map! {
                "title" => Value::text(title),
                "marks" => Value::text(marks_str),
                "visible_marks" => Value::text(visible_marks_str),
            });
        }

        api.set_widget(widget)?;
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

//! Sway colors
//!
//! Sway colors are picked up using sway IPC for the current bar.
//!
//! These are the available sway colors:
//!
//! * `active_workspace_bg`
//! * `active_workspace_border`
//! * `active_workspace_text`
//! * `background`
//! * `binding_mode_bg`
//! * `binding_mode_border`
//! * `binding_mode_text`
//! * `focused_background`
//! * `focused_separator`
//! * `focused_statusline`
//! * `focused_workspace_bg`
//! * `focused_workspace_border`
//! * `focused_workspace_text`
//! * `inactive_workspace_bg`
//! * `inactive_workspace_border`
//! * `inactive_workspace_text`
//! * `separator`
//! * `statusline`
//! * `urgent_workspace_bg`
//! * `urgent_workspace_border`
//! * `urgent_workspace_text`
//!
//! You can see the current values for your bar with `swaymsg -t get_bar_config <bar-name>`.

use swayipc::ColorableBarPart;

use std::sync::LazyLock;

use crate::errors::*;

#[cfg(not(test))]
use std::{fs::read_to_string, os::unix::process::parent_id, path::Path};

#[cfg(not(test))]
static COLORS: LazyLock<Result<ColorableBarPart>> =
    LazyLock::new(|| {
        let mut swayipc_connection =
            swayipc::Connection::new().error("Failed to open swayipc connection")?;

        let mut current_pid = parent_id().to_string();
        while current_pid != "1" {
            let cmdline = read_to_string(Path::new("/proc").join(&current_pid).join("cmdline"))
                .or_error(|| format!("Failed to read /proc/{current_pid}/cmdline"))?;
            let cmdline_parts: Vec<_> = cmdline.split('\0').collect();
            if let Some(bar_id) = cmdline_parts
                .iter()
                .position(|&s| s == "-b" || s == "--bar_id")
                .and_then(|pos| cmdline_parts.get(pos + 1))
            {
                return Ok(swayipc_connection
                    .get_bar_config(bar_id)
                    .error("Failed to get swaybar config")?
                    .colors);
            }
            let stat = read_to_string(Path::new("/proc").join(&current_pid).join("stat"))
                .or_error(|| format!("Failed to read /proc/{current_pid}/stat"))?;

            current_pid =
                stat.split(' ').nth(3).unwrap().parse().or_error(|| {
                    format!("Failed to parse parent PID from /proc/{current_pid}/stat")
                })?;
        }
        Err(Error::new(
            "Unable to find swaybar process in parent process tree",
        ))
    });

#[cfg(test)]
use tests::COLORS;

pub fn get_color(name: &str) -> Result<String> {
    COLORS.as_ref().map_err(Clone::clone).and_then(|colors| {
        Ok(match name {
            "background" => colors.background.clone(),
            "statusline" => colors.statusline.clone(),
            "separator" => colors.separator.clone(),
            "focused_background" => colors.focused_background.clone(),
            "focused_statusline" => colors.focused_statusline.clone(),
            "focused_separator" => colors.focused_separator.clone(),
            "focused_workspace_border" => colors.focused_workspace_border.clone(),
            "focused_workspace_bg" => colors.focused_workspace_bg.clone(),
            "focused_workspace_text" => colors.focused_workspace_text.clone(),
            "inactive_workspace_border" => colors.inactive_workspace_border.clone(),
            "inactive_workspace_bg" => colors.inactive_workspace_bg.clone(),
            "inactive_workspace_text" => colors.inactive_workspace_text.clone(),
            "active_workspace_border" => colors.active_workspace_border.clone(),
            "active_workspace_bg" => colors.active_workspace_bg.clone(),
            "active_workspace_text" => colors.active_workspace_text.clone(),
            "urgent_workspace_border" => colors.urgent_workspace_border.clone(),
            "urgent_workspace_bg" => colors.urgent_workspace_bg.clone(),
            "urgent_workspace_text" => colors.urgent_workspace_text.clone(),
            "binding_mode_border" => colors.binding_mode_border.clone(),
            "binding_mode_bg" => colors.binding_mode_bg.clone(),
            "binding_mode_text" => colors.binding_mode_text.clone(),
            _ => {
                return Err(Error::new(format!(
                    "color '{}' is not a valid sway color name",
                    name
                )));
            }
        })
    })
}

#[cfg(test)]
mod tests {

    use super::*;

    pub(crate) static COLORS: LazyLock<Result<ColorableBarPart>> = LazyLock::new(|| {
        let json_str = include_str!("../../../testdata/sway_colors.json");
        serde_json::from_str(json_str).error("Failed to parse sway colors JSON")
    });

    #[test]
    fn test_deserializing_xcolors() {
        use crate::themes::color::*;
        let mut parsed_color = "sway:separator".parse::<Color>().unwrap();
        assert_eq!(
            parsed_color,
            Color::Rgba(Rgba {
                r: 0x93,
                g: 0xa1,
                b: 0xa1,
                a: 0xff
            })
        );
        parsed_color = "sway:focused_workspace_border".parse::<Color>().unwrap();
        assert_eq!(
            parsed_color,
            Color::Rgba(Rgba {
                r: 0x00,
                g: 0x88,
                b: 0xcc,
                a: 0xff,
            })
        );
    }
}

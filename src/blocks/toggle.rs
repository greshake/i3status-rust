//! A Toggle block
//!
//! You can add commands to be executed to disable the toggle (`command_off`), and to enable it
//! (`command_on`). If these command exit with a non-zero status, the block will not be toggled and
//! the block state will be changed to give a visual warning of the failure. You also need to
//! specify a command to determine the state of the toggle (`command_state`). When the command outputs
//! nothing, the toggle is disabled, otherwise enabled. By specifying the interval property you can
//! let the command_state be executed continuously.
//!
//! To run those commands, the shell form `$SHELL` environment variable is used. If such variable
//! is not presented, `sh` is used.
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `format` | A string to customise the output of this block. See below for available placeholders | `" $icon "`
//! `command_on` | Shell command to enable the toggle | Yes | N/A
//! `command_off` | Shell command to disable the toggle | Yes | N/A
//! `command_state` | Shell command to determine the state. Empty output => No, otherwise => Yes. | **Required**
//! `icon_on` | Icon override for the toggle button while on | `"toggle_on"`
//! `icon_off` | Icon override for the toggle button while off | `"toggle_off"`
//! `interval` | Update interval in seconds. If not set, `command_state` will run only on click. | None
//!
//! Placeholder   | Value                                       | Type   | Unit
//! --------------|---------------------------------------------|--------|-----
//! `icon`        | Icon based on toggle's state                | Icon   | -
//!
//! Action   | Default button
//! ---------|---------------
//! `toggle` | Left
//!
//! # Examples
//!
//! This is what can be used to toggle an external monitor configuration:
//!
//! ```toml
//! [[block]]
//! block = "toggle"
//! format = " $icon 4k "
//! command_state = "xrandr | grep 'DP1 connected 38' | grep -v eDP1"
//! command_on = "~/.screenlayout/4kmon_default.sh"
//! command_off = "~/.screenlayout/builtin.sh"
//! interval = 5
//! ```
//!
//! # Icons Used
//! - `toggle_off`
//! - `toggle_on`

use super::prelude::*;
use std::env;
use tokio::process::Command;

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub format: FormatConfig,
    pub command_on: String,
    pub command_off: String,
    pub command_state: String,
    #[serde(default)]
    pub icon_on: Option<String>,
    #[serde(default)]
    pub icon_off: Option<String>,
    #[serde(default)]
    pub interval: Option<u64>,
}

pub async fn run(config: Config, mut api: CommonApi) -> Result<()> {
    api.set_default_actions(&[(MouseButton::Left, None, "toggle")])
        .await?;

    let interval = config.interval.map(Duration::from_secs);
    let mut widget = Widget::new().with_format(config.format.with_default(" $icon ")?);

    let icon_on = config.icon_on.unwrap_or_else(|| "toggle_on".into());
    let icon_off = config.icon_off.unwrap_or_else(|| "toggle_off".into());

    let shell = env::var("SHELL").unwrap_or_else(|_| "sh".to_string());

    loop {
        // Check state
        let output = Command::new(&shell)
            .args(["-c", &config.command_state])
            .output()
            .await
            .error("Failed to run command_state")?;
        let is_toggled = !std::str::from_utf8(&output.stdout)
            .error("The output of command_state is invalid UTF-8")?
            .trim()
            .is_empty();

        widget.set_values(map!(
            "icon" => Value::icon(
                api.get_icon(if is_toggled { &icon_on } else { &icon_off })?
            )
        ));
        api.set_widget(widget.clone()).await?;

        // TODO: try not to duplicate code
        loop {
            match interval {
                Some(interval) => {
                    select! {
                        _ = sleep(interval) => break,
                        event = api.event() => match event {
                            UpdateRequest => break,
                            Action(a) if a == "toggle" => {
                                let cmd = if is_toggled {
                                    &config.command_off
                                } else {
                                    &config.command_on
                                };
                                let output = Command::new(&shell)
                                    .args(["-c", cmd])
                                    .output()
                                    .await
                                    .error("Failed to run command")?;
                                if output.status.success() {
                                    widget.state = State::Idle;
                                    break;
                                } else {
                                    widget.state = State::Critical;
                                }
                            }
                            _ => (),
                        }
                    }
                }
                None => match api.event().await {
                    UpdateRequest => break,
                    Action(a) if a == "toggle" => {
                        let cmd = if is_toggled {
                            &config.command_off
                        } else {
                            &config.command_on
                        };
                        let output = Command::new(&shell)
                            .args(["-c", cmd])
                            .output()
                            .await
                            .error("Failed to run command")?;
                        if output.status.success() {
                            widget.state = State::Idle;
                            break;
                        } else {
                            widget.state = State::Critical;
                        }
                    }
                    _ => (),
                },
            }
        }
    }
}

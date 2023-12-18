//! A Toggle block
//!
//! You can add commands to be executed to disable the toggle (`command_off`), and to enable it
//! (`command_on`). If these command exit with a non-zero status, the block will not be toggled and
//! the block state will be changed to `critical` to give a visual warning of the failure. You also need to
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
//! `command_on` | Shell command to enable the toggle | **Required**
//! `command_off` | Shell command to disable the toggle | **Required**
//! `command_state` | Shell command to determine the state. Empty output => No, otherwise => Yes. | **Required**
//! `icon_on` | Icon override for the toggle button while on | `"toggle_on"`
//! `icon_off` | Icon override for the toggle button while off | `"toggle_off"`
//! `interval` | Update interval in seconds. If not set, `command_state` will run only on click. | None
//! `state_on` | [`State`] (color) of this block while on | [idle][State::Idle]
//! `state_off` | [`State`] (color) of this block while off | [idle][State::Idle]
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
//! state_on = "good"
//! state_off = "warning"
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
    pub state_on: Option<State>,
    pub state_off: Option<State>,
}

async fn sleep_opt(dur: Option<Duration>) {
    match dur {
        Some(dur) => tokio::time::sleep(dur).await,
        None => std::future::pending().await,
    }
}

pub async fn run(config: &Config, api: &CommonApi) -> Result<()> {
    let mut actions = api.get_actions()?;
    api.set_default_actions(&[(MouseButton::Left, None, "toggle")])?;

    let interval = config.interval.map(Duration::from_secs);
    let mut widget = Widget::new().with_format(config.format.with_default(" $icon ")?);

    let icon_on = config.icon_on.as_deref().unwrap_or("toggle_on");
    let icon_off = config.icon_off.as_deref().unwrap_or("toggle_off");

    let shell = env::var("SHELL").unwrap_or_else(|_| "sh".to_string());

    loop {
        // Check state
        let output = Command::new(&shell)
            .args(["-c", &config.command_state])
            .output()
            .await
            .error("Failed to run command_state")?;
        let is_on = !std::str::from_utf8(&output.stdout)
            .error("The output of command_state is invalid UTF-8")?
            .trim()
            .is_empty();

        widget.set_values(map!(
            "icon" => Value::icon(
                if is_on { icon_on.to_string() } else { icon_off.to_string() }
            )
        ));
        if widget.state != State::Critical {
            widget.state = if is_on {
                config.state_on.unwrap_or(State::Idle)
            } else {
                config.state_off.unwrap_or(State::Idle)
            };
        }
        api.set_widget(widget.clone())?;

        loop {
            select! {
                _ = sleep_opt(interval) => break,
                _ = api.wait_for_update_request() => break,
                Some(action) = actions.recv() => match action.as_ref() {
                    "toggle" => {
                        let cmd = if is_on {
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
                            // Temporary; it will immediately be updated by the outer loop
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
    }
}

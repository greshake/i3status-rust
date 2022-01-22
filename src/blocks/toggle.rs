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
//! Key | Values | Required | Default
//! ----|--------|----------|--------
//! `text` | A label next to the icon | No | `""`
//! `command_on` | Shell command to enable the toggle | Yes | N/A
//! `command_off` | Shell command to disable the toggle | Yes | N/A
//! `command_state` | Shell command to determine the state. Empty output => No, otherwise => Yes. | Yes | N/A
//! `icon_on` | Icon override for the toggle button while on | No | `"toggle_on"`
//! `icon_off` | Icon override for the toggle button while off | No | `"toggle_off"`
//! `interval` | Update interval in seconds. If not set, `command_state` will run only on click. | No | None
//!
//! # Examples
//!
//! This is what can be used to toggle an external monitor configuration:
//!
//! ```toml
//! [[block]]
//! block = "toggle"
//! text = "4k"
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
pub struct ToggleConfig {
    #[serde(default)]
    text: Option<String>,
    command_on: String,
    command_off: String,
    command_state: String,
    #[serde(default)]
    icon_on: Option<String>,
    #[serde(default)]
    icon_off: Option<String>,
    #[serde(default)]
    interval: Option<u64>,
}

pub async fn run(config: toml::Value, mut api: CommonApi) -> Result<()> {
    let mut events = api.get_events().await?;
    let config = ToggleConfig::deserialize(config).config_error()?;
    let interval = config.interval.map(Duration::from_secs);

    if let Some(text) = config.text {
        api.set_text(text);
    }

    let icon_on = config.icon_on.unwrap_or_else(|| "toggle_on".into());
    let icon_off = config.icon_off.unwrap_or_else(|| "toggle_off".into());

    // Choose the shell in this priority:
    // 1) `SHELL` environment varialble
    // 2) `"sh"`
    let shell = env::var("SHELL").unwrap_or_else(|_| "sh".to_string());

    loop {
        // Check state
        let output = Command::new(&shell)
            .args(&["-c", &config.command_state])
            .output()
            .await
            .error("Failed to run command_state")?;
        let is_toggled = !std::str::from_utf8(&output.stdout)
            .error("The output of command_state is invalid UTF-8")?
            .trim()
            .is_empty();

        // Update widget
        api.set_icon(if is_toggled { &icon_on } else { &icon_off })?;
        api.flush().await?;

        // TODO: try not to duplicate code
        loop {
            match interval {
                Some(interval) => {
                    tokio::select! {
                        _ = sleep(interval) => break,
                        Some(BlockEvent::Click(click)) = events.recv() => {
                            if click.button == MouseButton::Left {
                                let cmd = if is_toggled {
                                    &config.command_off
                                } else {
                                    &config.command_on
                                };
                                let output = Command::new(&shell)
                                    .args(&["-c", cmd])
                                    .output()
                                    .await
                                    .error("Failed to run command")?;
                                if output.status.success() {
                                    api.set_state(State::Idle);
                                    break;
                                } else {
                                    api.set_state(State::Critical);
                                }
                            }
                        },
                    }
                }
                None => {
                    if let Some(BlockEvent::Click(click)) = events.recv().await {
                        if click.button == MouseButton::Left {
                            let cmd = if is_toggled {
                                &config.command_off
                            } else {
                                &config.command_on
                            };
                            let output = Command::new(&shell)
                                .args(&["-c", cmd])
                                .output()
                                .await
                                .error("Failed to run command")?;
                            if output.status.success() {
                                api.set_state(State::Idle);
                                break;
                            } else {
                                api.set_state(State::Critical);
                            }
                        }
                    }
                }
            }
        }
    }
}

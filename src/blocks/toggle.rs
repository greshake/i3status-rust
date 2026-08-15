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
//! - `toggle_off` (`$icon`)
//! - `toggle_on` (`$icon`)

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

pub(crate) fn prepare(config: &Config) -> Result<Arc<BlockPlan>> {
    let format = config.format.with_default(" $icon ")?;
    let icon_on = config.icon_on.clone().unwrap_or_else(|| "toggle_on".into());
    let icon_off = config
        .icon_off
        .clone()
        .unwrap_or_else(|| "toggle_off".into());
    BlockPlan::new(vec![
        OutputPlan::new("on", format.clone()).icon("icon", IconChoices::one(icon_on)),
        OutputPlan::new("off", format).icon("icon", IconChoices::one(icon_off)),
    ])
}

pub(crate) async fn run(config: &Config, api: &CommonApi, plan: &Arc<BlockPlan>) -> Result<()> {
    let mut actions = api.get_actions()?;
    api.set_default_actions(&[(MouseButton::Left, None, "toggle")])?;

    let interval = config.interval.map(Duration::from_secs);

    let output_on = plan.output("on")?;
    let output_off = plan.output("off")?;

    let shell = env::var("SHELL").unwrap_or_else(|_| "sh".to_string());
    let mut state = State::Idle;

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

        let output = if is_on { &output_on } else { &output_off };
        let mut widget = output.new_widget();
        widget.set_values(map!(
            "icon" => output.icon_value("icon")?
        ));
        if state != State::Critical {
            state = if is_on {
                config.state_on.unwrap_or(State::Idle)
            } else {
                config.state_off.unwrap_or(State::Idle)
            };
        }
        widget.state = state;
        api.set_widget(widget)?;

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
                            state = State::Idle;
                            break;
                        } else {
                            state = State::Critical;
                        }
                    }
                    _ => (),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(icon_on: Option<&str>, icon_off: Option<&str>) -> Config {
        Config {
            format: Default::default(),
            command_on: String::new(),
            command_off: String::new(),
            command_state: String::new(),
            icon_on: icon_on.map(Into::into),
            icon_off: icon_off.map(Into::into),
            interval: None,
            state_on: None,
            state_off: None,
        }
    }

    #[test]
    fn plan_uses_default_icon_names() {
        let plan = prepare(&config(None, None)).unwrap();
        assert_eq!(
            plan.output("on").unwrap().single_icon("icon").unwrap(),
            "toggle_on"
        );
        assert_eq!(
            plan.output("off").unwrap().single_icon("icon").unwrap(),
            "toggle_off"
        );
    }

    #[test]
    fn plan_uses_configuration_derived_icon_names() {
        let plan = prepare(&config(Some("my_enabled_icon"), Some("my_disabled_icon"))).unwrap();
        assert_eq!(
            plan.output("on").unwrap().single_icon("icon").unwrap(),
            "my_enabled_icon"
        );
        assert_eq!(
            plan.output("off").unwrap().single_icon("icon").unwrap(),
            "my_disabled_icon"
        );
    }
}

use std::collections::HashMap;
use std::env;
use std::fmt::Debug;
use std::process::Command;
use std::time::Duration;

use crossbeam_channel::Sender;
use regex::Regex;
use serde::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::SharedConfig;
use crate::de::deserialize_opt_duration;
use crate::errors::*;
use crate::formatting::value::Value;
use crate::formatting::FormatTemplate;
use crate::protocol::i3bar_event::I3BarEvent;
use crate::scheduler::Task;
use crate::widgets::text::TextWidget;
use crate::widgets::{I3BarWidget, State};

pub struct SuperToggle {
    id: usize,
    text: TextWidget,
    command_on: String,
    command_off: String,
    command_current_state: String,
    format_on: FormatTemplate,
    format_off: FormatTemplate,
    command_status_on_regex: Regex,
    command_status_off_regex: Regex,
    icon_on: String,
    icon_off: String,
    update_interval: Option<Duration>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SuperToggleConfig {
    /// Update interval in seconds
    #[serde(default, deserialize_with = "deserialize_opt_duration")]
    pub interval: Option<Duration>,

    /// Shell Command to determine SuperToggle state.
    // #[serde(default = "SuperToggleConfig::default_command_current_state")]
    pub command_current_state: String,

    /// Shell Command to enable SuperToggle time tracking
    // #[serde(default = "SuperToggleConfig::default_command_on")]
    pub command_on: String,

    /// Shell Command to disable SuperToggle time tracking
    // #[serde(default = "SuperToggleConfig::default_command_off")]
    pub command_off: String,

    /// Format override
    pub format_on: FormatTemplate,

    /// Format override
    pub format_off: FormatTemplate,

    // #[serde(default = "SuperToggleConfig::default_command_status_on_regex")]
    #[serde(with = "serde_regex")]
    pub command_status_on_regex: Regex,

    // #[serde(default = "SuperToggleConfig::default_command_status_off_regex")]
    #[serde(with = "serde_regex")]
    pub command_status_off_regex: Regex,

    /// Icon ID when time tracking is on (default is "toggle_on")
    #[serde(default = "SuperToggleConfig::default_icon_on")]
    pub icon_on: String,

    /// Icon ID when time tracking is off (default is "toggle_off")
    #[serde(default = "SuperToggleConfig::default_icon_off")]
    pub icon_off: String,

    /// Text to display in i3bar for this block
    pub text: Option<String>,
}

impl SuperToggleConfig {
    fn default_icon_on() -> String {
        "toggle_on".to_owned()
    }

    fn default_icon_off() -> String {
        "toggle_off".to_owned()
    }
}

impl ConfigBlock for SuperToggle {
    type Config = SuperToggleConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        shared_config: SharedConfig,
        _tx_update_request: Sender<Task>,
    ) -> Result<Self> {
        Ok(SuperToggle {
            id,
            text: TextWidget::new(id, 0, shared_config)
                .with_text(&block_config.text.unwrap_or_default()),
            command_on: block_config.command_on,
            command_off: block_config.command_off,
            format_on: block_config.format_on.with_default("")?,
            format_off: block_config.format_off.with_default("")?,
            command_current_state: block_config.command_current_state,
            command_status_on_regex: block_config.command_status_on_regex,
            command_status_off_regex: block_config.command_status_off_regex,
            icon_on: block_config.icon_on,
            icon_off: block_config.icon_off,
            update_interval: block_config.interval,
        })
    }
}

fn get_output_of_command(command: &str) -> Result<String> {
    Command::new(env::var("SHELL").unwrap_or_else(|_| "sh".to_owned()))
        .args(&["-c", command])
        .output()
        .map(|o| Ok(String::from_utf8_lossy(&o.stdout).trim().to_owned()))?
}

fn get_mapped_matches_from_string<'a>(
    totest: &'a str,
    regex: &'a Regex,
) -> Option<HashMap<&'a str, Value>> {
    if let Some(captures) = regex.captures(totest) {
        let mut hash = HashMap::new();
        for name in regex.capture_names().flatten() {
            if let Some(value) = captures.name(name) {
                hash.insert(name, Value::from_string(value.as_str().to_owned()));
            }
        }
        return Some(hash);
    }
    None
}

impl SuperToggle {
    fn is_on_status_from_output(&self, output: &str) -> Result<bool> {
        if self.command_status_on_regex.is_match(output) {
            return Ok(true);
        }

        if self.command_status_off_regex.is_match(output) {
            return Ok(false);
        }

        Err(BlockError(
            "is_on_status".to_owned(),
            "Unable to match either the command_data_on or the command_data_off regex".to_owned(),
        ))
    }
}

impl Block for SuperToggle {
    fn update(&mut self) -> Result<Option<Update>> {
        let output = get_output_of_command(&self.command_current_state)?;

        let on = &self.is_on_status_from_output(&output)?;
        let tags_option = get_mapped_matches_from_string(
            &output,
            match on {
                true => &self.command_status_on_regex,
                false => &self.command_status_off_regex,
            },
        );

        match tags_option {
            Some(tags) => {
                self.text.set_icon(match on {
                    true => self.icon_on.as_str(),
                    false => self.icon_off.as_str(),
                })?;

                let output = match on {
                    true => self.format_on.render(&tags),
                    false => self.format_off.render(&tags),
                }?;

                self.text.set_texts(output);

                Ok(())
            }
            None => Err(BlockError(
                "update".to_owned(),
                "Unable to find a match on the command output".to_owned(),
            )),
        }?;

        self.text.set_state(State::Idle);

        Ok(self.update_interval.map(|d| d.into()))
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.text]
    }

    fn click(&mut self, _e: &I3BarEvent) -> Result<()> {
        let output = get_output_of_command(&self.command_current_state)?;
        let on = &self.is_on_status_from_output(&output)?;

        let cmd = match on {
            true => &self.command_off,
            false => &self.command_on,
        };

        let output =
            get_output_of_command(cmd).block_error("toggle", "Failed to run toggle command");

        if output.is_ok() {
            self.text.set_state(State::Idle);

            self.update()?;

            // Whatever we were, we are now the opposite, so set the icon appropriately
            self.text.set_icon(if !on {
                self.icon_on.as_str()
            } else {
                self.icon_off.as_str()
            })?
        } else {
            self.text.set_state(State::Critical);
        };

        Ok(())
    }

    fn id(&self) -> usize {
        self.id
    }
}

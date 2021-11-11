use std::process::Command;
use std::time::Duration;

use crossbeam_channel::Sender;
use regex::Regex;
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::SharedConfig;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::formatting::value::Value;
use crate::formatting::FormatTemplate;
use crate::scheduler::Task;
use crate::widgets::text::TextWidget;
use crate::widgets::{I3BarWidget, State};

pub struct Dnf {
    id: usize,
    output: TextWidget,
    update_interval: Duration,
    format: FormatTemplate,
    format_singular: FormatTemplate,
    format_up_to_date: FormatTemplate,
    warning_updates_regex: Option<Regex>,
    critical_updates_regex: Option<Regex>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct DnfConfig {
    // Update interval in seconds
    #[serde(deserialize_with = "deserialize_duration")]
    pub interval: Duration,

    /// Format override
    pub format: FormatTemplate,

    /// Alternative format override for when exactly 1 update is available
    pub format_singular: FormatTemplate,

    /// Alternative format override for when no updates are available
    pub format_up_to_date: FormatTemplate,

    /// Indicate a `warning` state for the block if any pending update match the
    /// following regex. Default behaviour is that no package updates are deemed
    /// warning
    pub warning_updates_regex: Option<String>,

    /// Indicate a `critical` state for the block if any pending update match the
    /// following regex. Default behaviour is that no package updates are deemed
    /// critical
    pub critical_updates_regex: Option<String>,
}

impl Default for DnfConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(600),
            format: FormatTemplate::default(),
            format_singular: FormatTemplate::default(),
            format_up_to_date: FormatTemplate::default(),
            warning_updates_regex: None,
            critical_updates_regex: None,
        }
    }
}

impl DnfConfig {
    fn unpack_regex(regex_str: Option<String>, errorstring: String) -> Result<Option<Regex>> {
        match regex_str {
            None => Ok(None),
            Some(s) => Regex::new(s.as_ref())
                .map_err(|_| ConfigurationError("dnf".to_string(), errorstring.to_string()))
                .map(Some),
        }
    }
}

impl ConfigBlock for Dnf {
    type Config = DnfConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        shared_config: SharedConfig,
        _tx_update_request: Sender<Task>,
    ) -> Result<Self> {
        let output = TextWidget::new(id, 0, shared_config).with_icon("update")?;

        Ok(Dnf {
            id,
            update_interval: block_config.interval,
            format: block_config.format.with_default("{count:1}")?,
            format_singular: block_config.format_singular.with_default("{count:1}")?,
            format_up_to_date: block_config.format_up_to_date.with_default("{count:1}")?,
            output,
            warning_updates_regex: DnfConfig::unpack_regex(
                block_config.warning_updates_regex,
                "invalid warning updates regex".to_owned(),
            )?,
            critical_updates_regex: DnfConfig::unpack_regex(
                block_config.critical_updates_regex,
                "invalid critical updates regex".to_owned(),
            )?,
        })
    }
}

fn get_updates_list() -> Result<String> {
    String::from_utf8(
        Command::new("sh")
            .env("LC_LANG", "C")
            .args(&["-c", "dnf check-update -q --skip-broken"])
            .output()
            .block_error("dnf", "Failure running dnf check-update")?
            .stdout,
    )
    .block_error("dnf", "Failed to capture dnf output")
}

fn get_update_count(updates: &str) -> usize {
    updates.lines().filter(|line| line.len() > 1).count()
}

fn has_warning_update(updates: &str, regex: &Regex) -> bool {
    updates.lines().filter(|line| regex.is_match(line)).count() > 0
}

fn has_critical_update(updates: &str, regex: &Regex) -> bool {
    updates.lines().filter(|line| regex.is_match(line)).count() > 0
}

impl Block for Dnf {
    fn id(&self) -> usize {
        self.id
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.output]
    }

    fn update(&mut self) -> Result<Option<Update>> {
        let (formatting_map, warning, critical, cum_count) = {
            let updates_list = get_updates_list()?;
            let count = get_update_count(&updates_list);
            let formatting_map = map!(
                "count" => Value::from_integer(count as i64)
            );

            let warning = self
                .warning_updates_regex
                .as_ref()
                .map_or(false, |regex| has_warning_update(&updates_list, regex));
            let critical = self
                .critical_updates_regex
                .as_ref()
                .map_or(false, |regex| has_critical_update(&updates_list, regex));

            (formatting_map, warning, critical, count)
        };
        self.output.set_texts(match cum_count {
            0 => self.format_up_to_date.render(&formatting_map)?,
            1 => self.format_singular.render(&formatting_map)?,
            _ => self.format.render(&formatting_map)?,
        });
        self.output.set_state(match cum_count {
            0 => State::Idle,
            _ => {
                if critical {
                    State::Critical
                } else if warning {
                    State::Warning
                } else {
                    State::Info
                }
            }
        });
        Ok(Some(self.update_interval.into()))
    }
}

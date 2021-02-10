use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::Write;
use std::process::Command;
use std::time::Duration;

use crossbeam_channel::Sender;
use regex::Regex;
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::Config;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::input::{I3BarEvent, MouseButton};
use crate::scheduler::Task;
use crate::util::FormatTemplate;
use crate::widget::{I3BarWidget, State};
use crate::widgets::button::ButtonWidget;

pub struct Apt {
    id: usize,
    output: ButtonWidget,
    update_interval: Duration,
    format: FormatTemplate,
    format_singular: FormatTemplate,
    format_up_to_date: FormatTemplate,
    warning_updates_regex: Option<Regex>,
    critical_updates_regex: Option<Regex>,
    config_path: String,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct AptConfig {
    /// Update interval in seconds
    #[serde(
        default = "AptConfig::default_interval",
        deserialize_with = "deserialize_duration"
    )]
    pub interval: Duration,

    /// Format override
    #[serde(default = "AptConfig::default_format")]
    pub format: String,

    /// Alternative format override for when exactly 1 update is available
    #[serde(default = "AptConfig::default_format")]
    pub format_singular: String,

    /// Alternative format override for when no updates are available
    #[serde(default = "AptConfig::default_format")]
    pub format_up_to_date: String,

    /// Indicate a `warning` state for the block if any pending update match the
    /// following regex. Default behaviour is that no package updates are deemed
    /// warning
    #[serde(default = "AptConfig::default_warning_updates_regex")]
    pub warning_updates_regex: Option<String>,

    /// Indicate a `critical` state for the block if any pending update match the following regex.
    /// Default behaviour is that no package updates are deemed critical
    #[serde(default = "AptConfig::default_critical_updates_regex")]
    pub critical_updates_regex: Option<String>,

    #[serde(default = "AptConfig::default_color_overrides")]
    pub color_overrides: Option<BTreeMap<String, String>>,
}

impl AptConfig {
    fn default_interval() -> Duration {
        Duration::from_secs(60 * 10)
    }

    fn default_format() -> String {
        "{count}".to_owned()
    }

    fn default_warning_updates_regex() -> Option<String> {
        None
    }

    fn default_critical_updates_regex() -> Option<String> {
        None
    }

    fn default_color_overrides() -> Option<BTreeMap<String, String>> {
        None
    }
}

impl ConfigBlock for Apt {
    type Config = AptConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        config: Config,
        _tx_update_request: Sender<Task>,
    ) -> Result<Self> {
        let mut cache_dir = env::temp_dir();
        cache_dir.push("i3rs-apt");
        if !cache_dir.exists() {
            fs::create_dir(cache_dir.clone()).block_error("apt", "Failed to create temp dir")?;
        }

        let apt_conf = format!(
            "Dir::State \"{}\";\n
             Dir::State::lists \"lists\";\n
             Dir::Cache \"{}\";\n
             Dir::Cache::srcpkgcache \"srcpkgcache.bin\";\n
             Dir::Cache::pkgcache \"pkgcache.bin\";",
            cache_dir.clone().into_os_string().into_string().unwrap(),
            cache_dir.clone().into_os_string().into_string().unwrap()
        );
        cache_dir.push("apt.conf");
        let mut config_file = fs::File::create(cache_dir.clone())
            .block_error("apt", "Failed to create config file")?;
        write!(config_file, "{}", apt_conf).block_error("apt", "Failed to write to config file")?;

        let output = ButtonWidget::new(config, id).with_icon("update");

        Ok(Apt {
            id,
            update_interval: block_config.interval,
            format: FormatTemplate::from_string(&block_config.format)
                .block_error("apt", "Invalid format specified for apt::format")?,
            format_singular: FormatTemplate::from_string(&block_config.format_singular)
                .block_error("apt", "Invalid format specified for apt::format_singular")?,
            format_up_to_date: FormatTemplate::from_string(&block_config.format_up_to_date)
                .block_error("apt", "Invalid format specified for apt::format_up_to_date")?,
            output,
            warning_updates_regex: match block_config.warning_updates_regex {
                None => None, // no regex configured
                Some(regex_str) => {
                    let regex = Regex::new(regex_str.as_ref()).map_err(|_| {
                        ConfigurationError(
                            "apt".to_string(),
                            (
                                "invalid warning updates regex".to_string(),
                                "invalid regex".to_string(),
                            ),
                        )
                    })?;
                    Some(regex)
                }
            },
            critical_updates_regex: match block_config.critical_updates_regex {
                None => None, // no regex configured
                Some(regex_str) => {
                    let regex = Regex::new(regex_str.as_ref()).map_err(|_| {
                        ConfigurationError(
                            "apt".to_string(),
                            (
                                "invalid critical updates regex".to_string(),
                                "invalid regex".to_string(),
                            ),
                        )
                    })?;
                    Some(regex)
                }
            },
            config_path: cache_dir.into_os_string().into_string().unwrap(),
        })
    }
}

fn has_warning_update(updates: &str, regex: &Regex) -> bool {
    updates.lines().filter(|line| regex.is_match(line)).count() > 0
}

fn has_critical_update(updates: &str, regex: &Regex) -> bool {
    updates.lines().filter(|line| regex.is_match(line)).count() > 0
}

fn get_updates_list(config_path: &str) -> Result<String> {
    // Update database
    Command::new("sh")
        .env("APT_CONFIG", config_path)
        .args(&["-c", "apt update"])
        .output()
        .block_error("apt", "Failed to run `apt update` command")?;

    String::from_utf8(
        Command::new("sh")
            .env("APT_CONFIG", config_path)
            .args(&["-c", "apt list --upgradable"])
            .output()
            .block_error("apt", "Problem running apt command")?
            .stdout,
    )
    .block_error("apt", "Problem capturing apt command output")
}

fn get_update_count(updates: &str) -> usize {
    updates
        .lines()
        .filter(|line| line.contains("[upgradable"))
        .count()
}

impl Block for Apt {
    fn id(&self) -> usize {
        self.id
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.output]
    }

    fn update(&mut self) -> Result<Option<Update>> {
        let (formatting_map, warning, critical, cum_count) = {
            let updates_list = get_updates_list(&self.config_path)?;
            let count = get_update_count(&updates_list);
            let formatting_map = map!("{count}" => count);

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
        self.output.set_text(match cum_count {
            0 => self.format_up_to_date.render_static_str(&formatting_map)?,
            1 => self.format_singular.render_static_str(&formatting_map)?,
            _ => self.format.render_static_str(&formatting_map)?,
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

    fn click(&mut self, event: &I3BarEvent) -> Result<()> {
        if event.matches_id(self.id) && event.button == MouseButton::Left {
            self.update()?;
        }
        Ok(())
    }
}

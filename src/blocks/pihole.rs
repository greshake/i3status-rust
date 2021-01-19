use std::collections::BTreeMap;
use std::process::Command;
use std::time::Duration;

use crossbeam_channel::Sender;
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::Config;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::input::{I3BarEvent, MouseButton};
use crate::scheduler::Task;
use crate::util::pseudo_uuid;
use crate::widget::I3BarWidget;
use crate::widgets::button::ButtonWidget;

pub struct PiHole {
    id: String,
    pihole: ButtonWidget,
    address: String,
    pwhash: String,
    update_interval: Duration,
    status: String,
}

impl PiHole {
    fn update_pihole_status(&mut self) -> Result<()> {
        let pihole_status = {
            let status_output = match Command::new("sh")
                .args(&[
                    "-c",
                    &format!(
                        r#"curl --max-time 3 --silent \
                        '{baseUrl}/admin/api.php?summary'"#,
                        baseUrl = self.address,
                    ),
                ])
                .output()
            {
                Ok(raw_output) => String::from_utf8(raw_output.stdout)
                    .block_error("pihole", "Failed to decode")?,
                Err(_) => String::from(""),
            };
            if status_output.is_empty() {
                (String::from("unreachable"), String::from("0"))
            } else {
                let status_json: serde_json::value::Value = serde_json::from_str(&status_output)
                    .block_error(
                        "pihole",
                        "Failed to parse JSON response from PiHole server.",
                    )?;
                let status_text = status_json
                    .pointer("/status")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or(String::from("unknown"));
                let ads_blocked_today = status_json
                    .pointer("/ads_blocked_today")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or(String::from("0"));

                let status: String;
                if status_text == "enabled" {
                    status = String::from("up");
                } else if status_text == "disabled" {
                    status = String::from("down");
                } else {
                    status = status_text
                }

                self.status = String::from(&status);

                (status, ads_blocked_today)
            }
        };

        if pihole_status.0 == "unreachable" {
            self.pihole.set_text("pi-hole is unreachable");
        } else {
            self.pihole.set_text(format!(
                "pi-hole {status} blocked {ads_blocked}",
                status = pihole_status.0,
                ads_blocked = pihole_status.1
            ));
        }
        Ok(())
    }

    fn toggle_pihole_status(&mut self) -> Result<()> {
        let command: String;
        if self.status == "up" {
            command = String::from("disable");
        } else if self.status == "down" {
            command = String::from("enable");
        } else {
            command = String::from("");
        }
        if !command.is_empty() {
            Command::new("sh")
                .args(&[
                    "-c",
                    &format!(
                        r#"curl --max-time 3 --silent \
                        '{baseUrl}/admin/api.php?{command}&auth={pwhash}'"#,
                        baseUrl = self.address,
                        command = &command,
                        pwhash = self.pwhash,
                    ),
                ])
                .output()
                .block_error("pihole", "Failed to toggle status")?;
            self.update_pihole_status()?;
        }
        Ok(())
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct PiHoleConfig {
    #[serde(
        default = "PiHoleConfig::default_interval",
        deserialize_with = "deserialize_duration"
    )]
    pub interval: Duration,
    #[serde(default = "PiHoleConfig::default_address")]
    pub address: String,
    pub pwhash: String,
    #[serde(default = "PiHoleConfig::default_color_overrides")]
    pub color_overrides: Option<BTreeMap<String, String>>,
}

impl PiHoleConfig {
    fn default_interval() -> Duration {
        Duration::from_secs(60)
    }

    fn default_address() -> String {
        String::from("http://pi.hole")
    }

    fn default_color_overrides() -> Option<BTreeMap<String, String>> {
        None
    }
}

impl ConfigBlock for PiHole {
    type Config = PiHoleConfig;

    fn new(
        block_config: Self::Config,
        config: Config,
        _tx_update_request: Sender<Task>,
    ) -> Result<Self> {
        let id = pseudo_uuid();
        Ok(PiHole {
            id: id.clone(),
            pihole: ButtonWidget::new(config, &id),
            address: block_config.address,
            pwhash: block_config.pwhash,
            update_interval: block_config.interval,
            status: String::from("unreachable"),
        })
    }
}

impl Block for PiHole {
    fn id(&self) -> &str {
        &self.id
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.pihole]
    }

    fn update(&mut self) -> Result<Option<Update>> {
        self.update_pihole_status()?;
        Ok(Some(self.update_interval.into()))
    }

    fn click(&mut self, event: &I3BarEvent) -> Result<()> {
        if event.matches_name(self.id()) {
            if let MouseButton::Left = event.button {
                self.toggle_pihole_status()?;
            }
        }
        Ok(())
    }
}

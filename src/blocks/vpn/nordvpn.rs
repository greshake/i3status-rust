use regex::Regex;
use std::process::Stdio;
use tokio::process::Command;

use crate::blocks::prelude::*;
use crate::util::country_flag_from_iso_code;

use super::{Driver, Status};

pub struct NordVpnDriver {
    regex_country_code: Regex,
}

impl NordVpnDriver {
    pub async fn new() -> NordVpnDriver {
        NordVpnDriver {
            regex_country_code: Regex::new("^.*Hostname:\\s+([a-z]{2}).*$").unwrap(),
        }
    }

    async fn run_network_command(arg: &str) -> Result<()> {
        Command::new("nordvpn")
            .args([arg])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .spawn()
            .error(format!("Problem running nordvpn command: {arg}"))?
            .wait()
            .await
            .error(format!("Problem running nordvpn command: {arg}"))?;
        Ok(())
    }

    async fn find_line(stdout: &str, needle: &str) -> Option<String> {
        stdout
            .lines()
            .find(|s| s.contains(needle))
            .map(|s| s.to_owned())
    }
}

#[async_trait]
impl Driver for NordVpnDriver {
    async fn get_status(&self) -> Result<Status> {
        let stdout = Command::new("nordvpn")
            .args(["status"])
            .output()
            .await
            .error("Problem running nordvpn command")?
            .stdout;

        let stdout = String::from_utf8(stdout).error("nordvpn produced non-UTF8 output")?;
        let line_status = Self::find_line(&stdout, "Status:").await;
        let line_country = Self::find_line(&stdout, "Country:").await;
        let line_country_flag = Self::find_line(&stdout, "Hostname:").await;
        if line_status.is_none() {
            return Ok(Status::Error);
        }
        let line_status = line_status.unwrap();

        if line_status.ends_with("Disconnected") {
            return Ok(Status::Disconnected);
        } else if line_status.ends_with("Connected") {
            let country = match line_country {
                Some(country_line) => country_line.rsplit(": ").next().unwrap().to_string(),
                None => String::default(),
            };
            let country_flag = match line_country_flag {
                Some(country_line_flag) => self
                    .regex_country_code
                    .captures_iter(&country_line_flag)
                    .last()
                    .map(|capture| capture[1].to_owned())
                    .map(|code| code.to_uppercase())
                    .map(|code| country_flag_from_iso_code(&code))
                    .unwrap_or_default(),
                None => String::default(),
            };
            return Ok(Status::Connected {
                country,
                country_flag,
            });
        }
        Ok(Status::Error)
    }

    async fn toggle_connection(&self, status: &Status) -> Result<()> {
        match status {
            Status::Connected { .. } => Self::run_network_command("disconnect").await?,
            Status::Disconnected => Self::run_network_command("connect").await?,
            Status::Error => (),
        }
        Ok(())
    }
}

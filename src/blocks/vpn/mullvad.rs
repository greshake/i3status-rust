use regex::Regex;
use std::process::Stdio;
use tokio::process::Command;

use crate::blocks::prelude::*;
use crate::util::country_flag_from_iso_code;

use super::{Driver, Status};

pub struct MullvadDriver {
    regex_country_code: Regex,
}

impl MullvadDriver {
    pub async fn new() -> MullvadDriver {
        MullvadDriver {
            regex_country_code: Regex::new("Connected to ([a-z]{2}).*, ([A-Z][a-z]*).*\n").unwrap(),
        }
    }

    async fn run_network_command(arg: &str) -> Result<()> {
        let code = Command::new("mullvad")
            .args([arg])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .spawn()
            .error(format!("Problem running mullvad command: {arg}"))?
            .wait()
            .await
            .error(format!("Problem running mullvad command: {arg}"))?;

        if code.success() {
            Ok(())
        } else {
            Err(Error::new(format!(
                "mullvad command failed with nonzero status: {code:?}"
            )))
        }
    }
}

#[async_trait]
impl Driver for MullvadDriver {
    async fn get_status(&self) -> Result<Status> {
        let stdout = Command::new("mullvad")
            .args(["status"])
            .output()
            .await
            .error("Problem running mullvad command")?
            .stdout;

        let status = String::from_utf8(stdout).error("mullvad produced non-UTF8 output")?;

        if status.contains("Disconnected") {
            return Ok(Status::Disconnected);
        } else if status.contains("Connected") {
            let (country_flag, country) = self
                .regex_country_code
                .captures_iter(&status)
                .next()
                .map(|capture| {
                    let country_code = capture[1].to_uppercase();
                    let country = capture[2].to_owned();
                    let country_flag = country_flag_from_iso_code(&country_code);
                    (country_flag, country)
                })
                .unwrap_or_default();

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

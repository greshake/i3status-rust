use std::process::Stdio;
use tokio::process::Command;

use crate::blocks::prelude::*;
use crate::util::country_flag_from_iso_code;

use super::{Driver, Status};

pub struct MullvadDriver {}

impl MullvadDriver {
    pub async fn new() -> MullvadDriver {
        MullvadDriver {}
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
            .args(["status", "-j"])
            .output()
            .await
            .error("Problem running mullvad command")?
            .stdout;

        let status: MullvadCliStatus =
            serde_json::from_slice(&stdout).error("'mullvad status' produced wrong JSON")?;

        match status {
            MullvadCliStatus::Disconnected => Ok(Status::Disconnected { profile: None }),
            MullvadCliStatus::Connected { details } => {
                let country_code = details
                    .location
                    .hostname
                    .map(|hostname| hostname[0..2].to_uppercase());

                let country_flag = country_code
                    .as_ref()
                    .map(|code| country_flag_from_iso_code(code));

                Ok(Status::Connected {
                    country: country_code,
                    country_flag,
                    profile: None,
                })
            }
            _ => Ok(Status::Error(None)),
        }
    }

    async fn toggle_connection(&self, status: &Status) -> Result<()> {
        match status {
            Status::Connected { .. } => Self::run_network_command("disconnect").await?,
            Status::Disconnected { .. } => Self::run_network_command("connect").await?,
            Status::Error(_) => (),
        }
        Ok(())
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "state")]
#[serde(rename_all = "snake_case")]
enum MullvadCliStatus {
    Connected { details: Details },

    Disconnected,

    Connecting,

    Disconnecting,
}

#[derive(Deserialize, Debug, Clone)]
struct Details {
    location: Location,
}

#[derive(Deserialize, Debug, Clone)]
struct Location {
    hostname: Option<String>,
}

use itertools::Itertools as _;
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
            // Sleep 1 sec here to allow the mullvad-daemon some time to update the status
            // before get_status is called again.
            sleep(Duration::from_secs(1)).await;
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
            .error("Problem running `mullvad status -j`")?
            .stdout;

        let json_status_output = String::from_utf8_lossy(&stdout);

        // As of 2026-04-29 there's a bug in the `mullvad status -j` command in that it sometimes
        // prints non-JSON data before the JSON output so we must filter that out here.
        let json_status = json_status_output
            .lines()
            .skip_while(|line| !line.starts_with("{"))
            .join("\n");

        let status: MullvadCliStatus =
            serde_json::from_str(&json_status).error("`mullvad status -j` produced wrong JSON")?;

        match status {
            MullvadCliStatus::Disconnected | MullvadCliStatus::Disconnecting => {
                Ok(Status::Disconnected { profile: None })
            }
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
            MullvadCliStatus::Connecting => Ok(Status::Connecting { profile: None }),
            MullvadCliStatus::Error { details } => {
                let error = details.and_then(|details| details.cause).and_then(|cause| {
                    if let Some(reason) = cause.reason
                        && let Some(details) = cause.details
                    {
                        Some(format!("{}: {}", reason, details))
                    } else {
                        None
                    }
                });

                Ok(Status::Error(error))
            }
        }
    }

    async fn toggle_connection(&self, status: &Status) -> Result<()> {
        match status {
            Status::Connected { .. } | Status::Connecting { .. } => {
                Self::run_network_command("disconnect").await?;
            }
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

    Error { details: Option<ErrorDetails> },
}

#[derive(Deserialize, Debug, Clone)]
struct Details {
    location: Location,
}

#[derive(Deserialize, Debug, Clone)]
struct Location {
    hostname: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
struct ErrorDetails {
    cause: Option<ErrorCause>,
}

#[derive(Deserialize, Debug, Clone)]
struct ErrorCause {
    reason: Option<String>,
    details: Option<String>,
}

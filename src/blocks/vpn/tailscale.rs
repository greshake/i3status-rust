use std::process::Stdio;
use tokio::process::Command;

use crate::blocks::prelude::*;

use super::{Driver, Status};

#[derive(Deserialize)]
struct CurrentTailnet {
    #[serde(rename = "Name")]
    name: String,
}

#[derive(Deserialize)]
struct TailscaleStatus {
    #[serde(rename = "BackendState")]
    backend_state: String,
    #[serde(rename = "CurrentTailnet")]
    current_tailnet: Option<CurrentTailnet>,
}

pub struct TailscaleDriver {}

impl TailscaleDriver {
    pub async fn new() -> Self {
        Self {}
    }

    async fn run_network_command(arg: &str) -> Result<()> {
        Command::new("tailscale")
            .args([arg])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .spawn()
            .error(format!("Problem running tailscale command: {arg}"))?
            .wait()
            .await
            .error(format!("Problem running tailscale command: {arg}"))?;
        Ok(())
    }
}

#[async_trait]
impl Driver for TailscaleDriver {
    async fn get_status(&self) -> Result<Status> {
        let cmd = Command::new("tailscale")
            .args(["status", "--json"])
            .output()
            .await
            .error("Problem running tailscale command")?;

        if !cmd.status.success() {
            let stderr =
                String::from_utf8(cmd.stderr).error("tailscale produced non-UTF8 stderr")?;
            if stderr.contains("it doesn't appear to be running") {
                return Ok(Status::Error);
            } else {
                return Err(Error::new(stderr));
            }
        }

        let stdout = String::from_utf8(cmd.stdout).error("tailscale produced non-UTF8 output")?;
        let status = serde_json::from_str::<TailscaleStatus>(&stdout)
            .error("Problem parsing tailscale status")?;
        let profile = status.current_tailnet.map(|t| t.name);
        match status.backend_state.as_str() {
            "Running" => Ok(Status::Connected {
                country: None,
                country_flag: None,
                profile,
            }),
            _ => Ok(Status::Disconnected { profile }),
        }
    }

    async fn toggle_connection(&self, status: &Status) -> Result<()> {
        match status {
            Status::Connected { .. } => Self::run_network_command("down").await?,
            Status::Disconnected { .. } => Self::run_network_command("up").await?,
            Status::Error => (),
        }
        Ok(())
    }
}

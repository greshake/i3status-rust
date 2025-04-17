use std::process::Stdio;
use tokio::process::Command;

use super::{Driver, Status};
use crate::blocks::prelude::*;

pub struct WarpDriver;

impl WarpDriver {
    pub async fn new() -> WarpDriver {
        WarpDriver
    }

    async fn run_network_command(arg: &str) -> Result<()> {
        Command::new("warp-cli")
            .args([arg])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .spawn()
            .error(format!("Problem running warp-cli command: {arg}"))?
            .wait()
            .await
            .error(format!("Problem running warp-cli command: {arg}"))?;
        Ok(())
    }
}

#[async_trait]
impl Driver for WarpDriver {
    async fn get_status(&self) -> Result<Status> {
        let stdout = Command::new("warp-cli")
            .args(["status"])
            .output()
            .await
            .error("Problem running warp-cli command")?
            .stdout;

        let status = String::from_utf8(stdout).error("warp-cli produced non-UTF8 output")?;

        if status.contains("Status update: Disconnected") {
            return Ok(Status::Disconnected);
        } else if status.contains("Status update: Connected") {
            return Ok(Status::Connected {
                country: "".to_string(), // because warp-cli doesn't provide country/server info
                country_flag: "".to_string(), // no country means no flag
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

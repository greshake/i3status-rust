use std::process::Stdio;

use async_trait::async_trait;
use nix::unistd::getuid;
use tokio::process::Command;

use crate::blocks::prelude::*;

use super::{Driver, Status};

pub struct WireguardDriver {
    interface: String,
}

impl WireguardDriver {
    pub async fn new(interface: String) -> WireguardDriver {
        WireguardDriver { interface }
    }
}

const SUDO_CMD: &str = "/usr/bin/sudo";
const WG_QUICK_CMD: &str = "/usr/bin/wg-quick";
const WG_CMD: &str = "/usr/bin/wg";

#[async_trait]
impl Driver for WireguardDriver {
    async fn get_status(&self) -> Result<Status> {
        let status = run_wg(&["show", self.interface.as_str()]).await;

        match status {
            Ok(status) => {
                if status.contains(format!("interface: {}", self.interface).as_str()) {
                    Ok(Status::Connected)
                } else {
                    Ok(Status::Disconnected)
                }
            }
            Err(_) => Ok(Status::Error),
        }
    }

    async fn toggle_connection(&self, status: &Status) -> Result<()> {
        match status {
            Status::Connected | Status::ConnectedToCountry { .. } => {
                run_wg_quick(&["down", self.interface.as_str()]).await?;
            }
            Status::Disconnected => {
                run_wg_quick(&["up", self.interface.as_str()]).await?;
            }
            Status::Error => (),
        }
        Ok(())
    }
}

async fn run_wg(args: &[&str]) -> Result<String> {
    let stdout = make_command(should_use_sudo(), WG_CMD)
        .args(args)
        .output()
        .await
        .error(format!("Problem running wg command: {args:?}"))?
        .stdout;
    let stdout =
        String::from_utf8(stdout).error(format!("wg produced non-UTF8 output: {args:?}"))?;
    Ok(stdout)
}

async fn run_wg_quick(args: &[&str]) -> Result<()> {
    make_command(should_use_sudo(), WG_QUICK_CMD)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .spawn()
        .error(format!("Problem running wg-quick command: {args:?}"))?
        .wait()
        .await
        .error(format!("Problem running wg-quick command: {args:?}"))?;
    Ok(())
}

fn make_command(use_sudo: bool, cmd: &str) -> Command {
    let mut command = Command::new(if use_sudo { SUDO_CMD } else { cmd });

    if use_sudo {
        command.arg("-n").arg(cmd);
    }
    command
}

fn should_use_sudo() -> bool {
    !(getuid().is_root())
}

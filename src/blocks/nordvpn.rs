//! Current connection status for nordvpn networks
//!
//! Behind the scenes this uses the `nordvpn` command line binary. In order for this to work
//! properly the binary should be executable without root privileges.
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `interval` | Update interval in seconds. | `10`
//! `format_connected` | A string to customise the output in case the network is connected. See below for available placeholders. | `" VPN: $icon "`
//! `format_disconnected` | A string to customise the output in case the network is disconnected. See below for available placeholders. | `" VPN: $icon "`
//! `state_connected` | The widgets state if the nordvpn network is connected. | `info`
//! `state_disconnected` | The widgets state if the nordvpn network is disconnected | `idle`
//!
//! Placeholder | Value                          | Type   | Unit
//! ------------|--------------------------------|--------|------
//! `icon`      | A static icon                  | Icon   | -
//! `country`   | Country currently connected to | Text   | -
//!
//! # Example
//!
//! Shows the current nordvpn network state:
//!
//! ```toml
//! [[block]]
//! block = "nordvpn"
//! interval = 10
//! format_connected = "VPN: $icon "
//! format_disconnected = "VPN: $icon "
//! state_connected = "info"
//! state_diconnected = "idle"
//! ```
//!
//! Possible values for `state_connected` and `state_diconnected`:
//!
//! ```
//! warning
//! critical
//! info
//! idle
//! ```
//! `Option::None` defaults to `idle`.
//!
//! # Icons Used
//!
//! - `net_vpn`
//! - `net_wired`
//! - `net_down`

use core::str::FromStr;
use std::process::Stdio;
use tokio::process::Command;

use super::prelude::*;

#[derive(Deserialize, Debug)]
#[serde(default)]
pub struct Config {
    interval: Seconds,
    format_connected: FormatConfig,
    format_disconnected: FormatConfig,
    state_connected: Option<String>,
    state_disconnected: Option<String>,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            interval: 10.into(),
            format_connected: FormatConfig::from_str(" VPN: $icon ").unwrap(),
            format_disconnected: FormatConfig::from_str(" VPN: $icon ").unwrap(),
            state_connected: Option::from(String::from("info")),
            state_disconnected: Option::from(String::from("idle")),
        }
    }
}

enum Status {
    Connected(String),
    Disconnected,
    Error,
}

impl Status {
    fn icon(&self) -> &str {
        match *self {
            Status::Connected(_) => "net_vpn",
            Status::Disconnected => "net_wired",
            Status::Error => "net_down",
        }
    }
}

pub async fn run(config: Config, mut api: CommonApi) -> Result<()> {
    api.set_default_actions(&[(MouseButton::Left, None, "toggle")])
        .await?;

    let mut widget = Widget::new();

    let format_connected = config.format_connected.with_default(" VPN: $icon ")?;
    let format_disconnected = config.format_disconnected.with_default(" VPN: $icon ")?;

    loop {
        let status = get_current_network_status().await?;

        widget.set_values(map!(
            "icon" => Value::icon(api.get_icon(status.icon())?),
            "country" => match &status {
               Status::Connected(country)  => Value::text(country.clone()),
               _ => Value::text(String::default()),
            }
        ));

        let state_connected = config.state_connected.as_deref().unwrap_or("info");
        let state_disconnected = config.state_disconnected.as_deref().unwrap_or("idle");

        widget.state = match status {
            Status::Connected(_) => {
                widget.set_format(format_connected.clone());
                State::from_str(state_connected).unwrap()
            }
            Status::Disconnected => {
                widget.set_format(format_disconnected.clone());
                State::from_str(state_disconnected).unwrap()
            }
            Status::Error => {
                widget.set_format(format_disconnected.clone());
                State::Critical
            }
        };

        api.set_widget(&widget).await?;

        select! {
            _ = sleep(config.interval.0) => (),
            event = api.event() => {
                match event {
                    Action(a) if a == "toggle" => {
                        match status {
                            Status::Connected(_) => run_network_command("disconnect").await?,
                            Status::Disconnected => run_network_command("connect").await?,
                            Status::Error => (),
                        }
                    }
                    _ => (),
                }
            }
        }
    }
}

async fn get_current_network_status() -> Result<Status> {
    let stdout = Command::new("nordvpn")
        .args(["status"])
        .output()
        .await
        .error("Problem running nordvpn command")?
        .stdout;

    let stdout = String::from_utf8(stdout).error("nordvpn produced non-UTF8 output")?;
    let line_status = find_line(&stdout, "Status:").await;
    let line_country = find_line(&stdout, "Country:").await;
    if line_status.is_none() {
        return Ok(Status::Error);
    }
    let line_status = line_status.unwrap();

    if line_status.ends_with("Disconnected") {
        return Ok(Status::Disconnected);
    } else if line_status.ends_with("Connected") {
        match line_country {
            Some(country_line) => {
                let country = country_line
                    .split(": ")
                    .collect::<Vec<&str>>()
                    .last()
                    .unwrap()
                    .to_string();
                return Ok(Status::Connected(country));
            }
            None => return Ok(Status::Connected(String::default())),
        }
    }
    Ok(Status::Error)
}

async fn find_line(stdout: &str, needle: &str) -> Option<String> {
    stdout
        .lines()
        .find(|s| s.contains(needle))
        .map(|s| s.to_owned())
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

impl FromStr for State {
    type Err = ();

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "info" => Ok(State::Info),
            "warning" => Ok(State::Warning),
            "critical" => Ok(State::Critical),
            _ => Ok(State::Idle),
        }
    }
}

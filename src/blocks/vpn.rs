//! Current connection status for vpn networks
//! This widget toggles the connection on left click.
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `driver` | Which vpn should be used . Available drivers are: `"nordvpn"` | `"nordvpn"`
//! `interval` | Update interval in seconds. | `10`
//! `format_connected` | A string to customise the output in case the network is connected. See below for available placeholders. | `" VPN: $icon "`
//! `format_disconnected` | A string to customise the output in case the network is disconnected. See below for available placeholders. | `" VPN: $icon "`
//! `state_connected` | The widgets state if the vpn network is connected. | `info`
//! `state_disconnected` | The widgets state if the vpn network is disconnected | `idle`
//!
//! Placeholder | Value                                                     | Type   | Unit
//! ------------|-----------------------------------------------------------|--------|------
//! `icon`      | A static icon                                             | Icon   | -
//! `country`   | Country currently connected to                            | Text   | -
//! `flag`      | Country specific flag (depends on a font supporting them) | Text   | -
//!
//! # Drivers
//!
//! ## nordvpn
//! Behind the scenes the nordvpn driver uses the `nordvpn` command line binary. In order for this to work
//! properly the binary should be executable without root privileges.
//!
//! # Example
//!
//! Shows the current vpn network state:
//!
//! ```toml
//! [[block]]
//! block = "vpn"
//! driver = "nordvpn"
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
//! - country code flags (if supported by font)
//!
//! Flags: They are not icons but unicode glyphs. You will need a font that
//! includes them. Tested with: <https://www.babelstone.co.uk/Fonts/Flags.html>

use core::str::FromStr;
use regex::Regex;
use std::process::Stdio;
use tokio::process::Command;

use crate::util::country_flag_from_iso_code;

use super::prelude::*;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(rename_all = "snake_case")]
enum DriverType {
    #[default]
    Nordvpn,
}

#[derive(Deserialize, Debug)]
#[serde(default)]
pub struct Config {
    driver: DriverType,
    interval: Seconds,
    format_connected: FormatConfig,
    format_disconnected: FormatConfig,
    state_connected: Option<String>,
    state_disconnected: Option<String>,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            driver: DriverType::Nordvpn,
            interval: 10.into(),
            format_connected: FormatConfig::from_str(" VPN: $icon ").unwrap(),
            format_disconnected: FormatConfig::from_str(" VPN: $icon ").unwrap(),
            state_connected: Option::from(String::from("info")),
            state_disconnected: Option::from(String::from("idle")),
        }
    }
}

enum Status {
    Connected {
        country: String,
        country_flag: String,
    },
    Disconnected,
    Error,
}

impl Status {
    fn icon(&self) -> &str {
        match self {
            Status::Connected { .. } => "net_vpn",
            Status::Disconnected => "net_wired",
            Status::Error => "net_down",
        }
    }

    fn flag(&self) -> &str {
        match self {
            Status::Connected { country_flag, .. } => country_flag.as_str(),
            _ => "",
        }
    }

    fn country(&self) -> &str {
        match self {
            Status::Connected { country, .. } => country.as_str(),
            _ => "",
        }
    }
}

pub async fn run(config: Config, mut api: CommonApi) -> Result<()> {
    api.set_default_actions(&[(MouseButton::Left, None, "toggle")])
        .await?;

    let mut widget = Widget::new();

    let format_connected = config.format_connected.with_default(" VPN: $icon ")?;
    let format_disconnected = config.format_disconnected.with_default(" VPN: $icon ")?;

    let driver: Box<dyn Driver> = match config.driver {
        DriverType::Nordvpn => Box::new(NordVpnDriver::new().await),
    };

    loop {
        let status = driver.get_status().await?;

        widget.set_values(map!(
            "icon" => Value::icon(api.get_icon(status.icon())?),
            "country" => Value::text(status.country().to_string()),
            "flag" => Value::text(status.flag().to_string()),

        ));

        let state_connected = config.state_connected.as_deref().unwrap_or("info");
        let state_disconnected = config.state_disconnected.as_deref().unwrap_or("idle");

        widget.state = match status {
            Status::Connected { .. } => {
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
                    Action(a) if a == "toggle" => driver.toggle_connection(&status).await?,
                    _ => (),
                }
            }
        }
    }
}

#[async_trait]
trait Driver {
    async fn get_status(&self) -> Result<Status>;
    async fn toggle_connection(&self, status: &Status) -> Result<()>;
}

struct NordVpnDriver {
    regex_counry_code: Regex,
}

impl NordVpnDriver {
    async fn new() -> NordVpnDriver {
        NordVpnDriver {
            regex_counry_code: Regex::new("^.*Hostname:\\s+([a-z]{2}).*$").unwrap(),
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
                    .regex_counry_code
                    .captures_iter(&country_line_flag)
                    .last()
                    .map(|capture| capture[1].to_owned())
                    .map(|code| code.to_uppercase())
                    .map(|code| country_flag_from_iso_code(&code))
                    .unwrap_or(String::default()),
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

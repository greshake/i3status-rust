//! Shows the current connection status for VPN networks
//!
//! This widget toggles the connection on left click.
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `driver` | Which vpn should be used . Available drivers are: `"nordvpn"`, `"mullvad"`, `"tailscale"` | `"nordvpn"`
//! `interval` | Update interval in seconds. | `10`
//! `format_connected` | A string to customise the output in case the network is connected. See below for available placeholders. | `" VPN: $icon "`
//! `format_disconnected` | A string to customise the output in case the network is disconnected. See below for available placeholders. | `" VPN: $icon "`
//! `format_connecting` | A string to customise the output in case the network is in a connecting state. See below for available placeholders. | `" VPN: $icon "`
//! `state_connected` | The widgets state if the vpn network is connected. | `info`
//! `state_disconnected` | The widgets state if the vpn network is disconnected | `idle`
//!
//! Placeholder | Value                                                     | Type   | Unit
//! ------------|-----------------------------------------------------------|--------|------
//! `icon`      | A static icon                                             | Icon   | -
//! `country`   | Country currently connected to                            | Text   | -
//! `flag`      | Country specific flag (depends on a font supporting them) | Text   | -
//! `profile`   | Currently selected profile configuration (tailnet)        | Text   | -
//! `error`     | Error message if any                                      | Text   | -
//!
//! Action    | Default button | Description
//! ----------|----------------|-----------------------------------
//! `toggle`  | Left           | toggles the vpn network connection
//!
//! # Drivers
//!
//! ## Mullvad
//! Behind the scenes the mullvad driver uses the `mullvad` command line binary. In order for this to work properly the binary should be executable and mullvad daemon should be running.
//!
//! ## nordvpn
//! Behind the scenes the nordvpn driver uses the `nordvpn` command line binary. In order for this to work
//! properly the binary should be executable without root privileges.
//!
//! ## Tailscale
//! Behind the scenes the tailscale driver uses the `tailscale` command line binary.
//! In order for this to work properly the tailscale daemon should be running and the user must be configured as operator:
//! ```sh
//! sudo tailscale set --operator=$USER
//! ```
//!
//! ## Cloudflare WARP
//! Behind the scenes the WARP driver uses the `warp-cli` command line binary. Just ensure the binary is executable without root privileges.
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
//! format_connecting = "VPN: $icon "
//! state_connected = "good"
//! state_disconnected = "warning"
//! ```
//!
//! Possible values for `state_connected` and `state_disconnected`:
//!
//! ```text
//! warning
//! critical
//! good
//! info
//! idle
//! ```
//!
//! # Icons Used
//!
//! - `net_vpn` (`$icon`, in `format_connected`)
//! - `net_wired` (`$icon`, in `format_disconnected`)
//! - `net_wireless` (`$icon`, in `format_connecting`)
//! - `net_down` (`$icon`, in `format_disconnected`, on errors)
//! - country code flags (if supported by font)
//!
//! Flags: They are not icons but unicode glyphs. You will need a font that
//! includes them. Tested with: <https://www.babelstone.co.uk/Fonts/Flags.html>

mod mullvad;
use mullvad::MullvadDriver;
mod nordvpn;
use nordvpn::NordVpnDriver;
mod tailscale;
use tailscale::TailscaleDriver;
mod warp;
use warp::WarpDriver;

use super::prelude::*;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(rename_all = "snake_case")]
pub enum DriverType {
    Mullvad,
    #[default]
    Nordvpn,
    Tailscale,
    Warp,
}

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub driver: DriverType,
    #[default(10.into())]
    pub interval: Seconds,
    pub format_connected: FormatConfig,
    pub format_disconnected: FormatConfig,
    pub format_connecting: FormatConfig,
    pub state_connected: State,
    pub state_disconnected: State,
}

enum Status {
    Connected {
        country: Option<String>,
        country_flag: Option<String>,
        profile: Option<String>,
    },
    Connecting {
        profile: Option<String>,
    },
    Disconnected {
        profile: Option<String>,
    },
    Error(Option<String>),
}

impl DriverType {
    /// Whether the driver's `get_status` can ever report `Connecting`.
    /// Only mullvad has an intermediate connecting state; the other CLIs
    /// report connected, disconnected, or an error.
    fn can_report_connecting(&self) -> bool {
        matches!(self, DriverType::Mullvad)
    }
}

pub(crate) fn prepare(config: &Config) -> Result<Arc<BlockPlan>> {
    let mut outputs = vec![
        OutputPlan::new(
            "connected",
            config.format_connected.with_default(" VPN: $icon ")?,
        )
        .icon("icon", IconChoices::one("net_vpn")),
        OutputPlan::new(
            "disconnected",
            config.format_disconnected.with_default(" VPN: $icon ")?,
        )
        .icon("icon", IconChoices::one("net_wired")),
    ];
    if config.driver.can_report_connecting() {
        outputs.push(
            OutputPlan::new(
                "connecting",
                config.format_connecting.with_default(" VPN: $icon ")?,
            )
            .icon("icon", IconChoices::one("net_wireless")),
        );
    }
    outputs.push(
        OutputPlan::new(
            "error",
            config.format_disconnected.with_default(" VPN: $icon ")?,
        )
        .icon("icon", IconChoices::one("net_down")),
    );
    BlockPlan::new(outputs)
}

pub(crate) async fn run(config: &Config, api: &CommonApi, plan: &Arc<BlockPlan>) -> Result<()> {
    let mut actions = api.get_actions()?;
    api.set_default_actions(&[(MouseButton::Left, None, "toggle")])?;

    let output_connected = plan.output("connected")?;
    let output_disconnected = plan.output("disconnected")?;
    let output_connecting = if config.driver.can_report_connecting() {
        Some(plan.output("connecting")?)
    } else {
        None
    };
    let output_error = plan.output("error")?;

    let driver: Box<dyn Driver> = match config.driver {
        DriverType::Mullvad => Box::new(MullvadDriver::new().await),
        DriverType::Nordvpn => Box::new(NordVpnDriver::new().await),
        DriverType::Tailscale => Box::new(TailscaleDriver::new().await),
        DriverType::Warp => Box::new(WarpDriver::new().await),
    };

    loop {
        let status = driver.get_status().await?;

        let output = match &status {
            Status::Connected { .. } => &output_connected,
            Status::Disconnected { .. } => &output_disconnected,
            // A driver reporting a state outside its declared capability is
            // an internal bug; degrade to the disconnected output.
            Status::Connecting { .. } => output_connecting.as_ref().unwrap_or_else(|| {
                debug_assert!(false, "driver reported Connecting without the capability");
                &output_disconnected
            }),
            Status::Error(_) => &output_error,
        };
        let mut widget = output.new_widget();
        let icon = output.icon_value("icon")?;

        widget.state = match &status {
            Status::Connected {
                country,
                country_flag,
                profile,
            } => {
                widget.set_values(map!(
                        "icon" => icon.clone(),
                        [if let Some(country) = country] "country" => Value::text(country.into()),
                        [if let Some(flag) = country_flag] "flag" => Value::text(flag.into()),
                        [if let Some(profile) = profile] "profile" => Value::text(profile.into()),
                ));
                config.state_connected
            }
            Status::Disconnected { profile } => {
                widget.set_values(map! {
                    "icon" => icon.clone(),
                    [if let Some(profile) = profile] "profile" => Value::text(profile.into()),
                });
                config.state_disconnected
            }
            Status::Connecting { profile } => {
                widget.set_values(map!(
                        "icon" => icon.clone(),
                        [if let Some(profile) = profile] "profile" => Value::text(profile.into()),
                ));
                State::Info
            }
            Status::Error(error) => {
                widget.set_values(map!(
                        "icon" => icon.clone(),
                        [if let Some(error) = error] "error" => Value::text(error.into())
                ));
                State::Critical
            }
        };

        api.set_widget(widget)?;

        select! {
            _ = sleep(config.interval.0) => (),
            _ = api.wait_for_update_request() => (),
            Some(action) = actions.recv() => match action.as_ref() {
                "toggle" => driver.toggle_connection(&status).await?,
                _ => (),
            }
        }
    }
}

#[async_trait]
trait Driver {
    async fn get_status(&self) -> Result<Status>;
    async fn toggle_connection(&self, status: &Status) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_declares_states_reachable_for_the_driver() {
        // Default driver is nordvpn, which never reports Connecting.
        let plan = prepare(&Config::default()).unwrap();
        let declared: Vec<_> = plan.outputs().map(|o| o.id()).collect();
        assert_eq!(declared, ["connected", "disconnected", "error"]);

        let mullvad = Config {
            driver: DriverType::Mullvad,
            ..Config::default()
        };
        let plan = prepare(&mullvad).unwrap();
        let declared: Vec<_> = plan.outputs().map(|o| o.id()).collect();
        assert_eq!(
            declared,
            ["connected", "disconnected", "connecting", "error"]
        );
        for (id, icon) in [
            ("connected", "net_vpn"),
            ("disconnected", "net_wired"),
            ("connecting", "net_wireless"),
            ("error", "net_down"),
        ] {
            let output = plan.output(id).unwrap();
            let choices = output.output().choices_for("icon").unwrap();
            assert!(choices.permits(icon), "{id} must permit {icon}");
        }
    }

    #[test]
    fn each_output_declares_exactly_one_icon() {
        let plan = prepare(&Config::default()).unwrap();
        for (id, icon) in [
            ("connected", "net_vpn"),
            ("disconnected", "net_wired"),
            ("error", "net_down"),
        ] {
            let output = plan.output(id).unwrap();
            assert_eq!(output.single_icon("icon").unwrap(), icon);
        }
    }

    #[test]
    fn error_state_uses_disconnected_format() {
        let config = Config {
            format_disconnected: " off ".parse().unwrap(),
            ..Config::default()
        };
        let plan = prepare(&config).unwrap();
        let error = plan.output("error").unwrap();
        assert!(!error.format().contains_key("icon"));
    }
}

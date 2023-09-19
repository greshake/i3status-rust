//! Shows the current connection status for VPN networks
//!
//! This widget toggles the connection on left click.
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `driver` | Which vpn should be used . Available drivers are: `"nordvpn"` and `"mullvad"` | `"nordvpn"`
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
//! Action    | Default button | Description
//! ----------|----------------|-----------------------------------
//! `toggle`  | Left           | toggles the vpn network connection
//!
//! # Drivers
//!
//! ## nordvpn
//! Behind the scenes the nordvpn driver uses the `nordvpn` command line binary. In order for this to work
//! properly the binary should be executable without root privileges.
//!
//! ## Mullvad
//! Behind the scenes the mullvad driver uses the `mullvad` command line binary. In order for this to work properly the binary should be executable and mullvad daemon should be running.
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
//! - `net_vpn`
//! - `net_wired`
//! - `net_down`
//! - country code flags (if supported by font)
//!
//! Flags: They are not icons but unicode glyphs. You will need a font that
//! includes them. Tested with: <https://www.babelstone.co.uk/Fonts/Flags.html>

mod nordvpn;
use nordvpn::NordVpnDriver;
mod mullvad;
use mullvad::MullvadDriver;

use super::prelude::*;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(rename_all = "snake_case")]
pub enum DriverType {
    #[default]
    Nordvpn,
    Mullvad,
}

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub driver: DriverType,
    #[default(10.into())]
    pub interval: Seconds,
    pub format_connected: FormatConfig,
    pub format_disconnected: FormatConfig,
    pub state_connected: State,
    pub state_disconnected: State,
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
    fn icon(&self) -> Cow<'static, str> {
        match self {
            Status::Connected { .. } => "net_vpn".into(),
            Status::Disconnected => "net_wired".into(),
            Status::Error => "net_down".into(),
        }
    }
}

pub async fn run(config: &Config, api: &CommonApi) -> Result<()> {
    let mut actions = api.get_actions()?;
    api.set_default_actions(&[(MouseButton::Left, None, "toggle")])?;

    let format_connected = config.format_connected.with_default(" VPN: $icon ")?;
    let format_disconnected = config.format_disconnected.with_default(" VPN: $icon ")?;

    let driver: Box<dyn Driver> = match config.driver {
        DriverType::Nordvpn => Box::new(NordVpnDriver::new().await),
        DriverType::Mullvad => Box::new(MullvadDriver::new().await),
    };

    loop {
        let status = driver.get_status().await?;

        let mut widget = Widget::new();

        widget.state = match &status {
            Status::Connected {
                country,
                country_flag,
            } => {
                widget.set_values(map!(
                        "icon" => Value::icon(status.icon()),
                        "country" => Value::text(country.to_string()),
                        "flag" => Value::text(country_flag.to_string()),

                ));
                widget.set_format(format_connected.clone());
                config.state_connected
            }
            Status::Disconnected => {
                widget.set_values(map!(
                        "icon" => Value::icon(status.icon()),
                ));
                widget.set_format(format_disconnected.clone());
                config.state_disconnected
            }
            Status::Error => {
                widget.set_values(map!(
                        "icon" => Value::icon(status.icon()),
                ));
                widget.set_format(format_disconnected.clone());
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

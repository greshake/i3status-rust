//! External IP address and various information about it
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `format` | A string to customise the output of this block. See below for available placeholders. | `" $ip $country_flag "`
//! `interval` | Interval in seconds for automatic updates | `300`
//! `with_network_manager` | If 'true', listen for NetworkManager events and update the IP immediately if there was a change | `true`
//! `use_ipv4` | If 'true', use IPv4 for obtaining all info | `false`
//!
//!  Key | Value | Type | Unit
//! -----|-------|------|------
//! `ip` | The external IP address, as seen from a remote server | Text | -
//! `version` | IPv4 or IPv6 | Text | -
//! `city` | City name, such as "San Francisco" | Text | -
//! `region` | Region name, such as "California" | Text | -
//! `region_code` | Region code, such as "CA" for California | Text | -
//! `country` | Country code (2 letter, ISO 3166-1 alpha-2) | Text | -
//! `country_name` | Short country name | Text | -
//! `country_code` | Country code (2 letter, ISO 3166-1 alpha-2) | Text | -
//! `country_code_iso3` | Country code (3 letter, ISO 3166-1 alpha-3) | Text | -
//! `country_capital` | Capital of the country | Text | -
//! `country_tld` | Country specific TLD (top-level domain) | Text | -
//! `continent_code` | Continent code | Text | -
//! `in_eu` | Region code, such as "CA" | Flag | -
//! `postal` | ZIP / Postal code | Text | -
//! `latitude` | Latitude | Number | - (TODO: make degrees?)
//! `longitude` | Longitude | Number | - (TODO: make degrees?)
//! `timezone` | City | Text | -
//! `utc_offset` | UTC offset (with daylight saving time) as +HHMM or -HHMM (HH is hours, MM is minutes) | Text | -
//! `country_calling_code` | Country calling code (dial in code, comma separated) | Text | -
//! `currency` | Currency code (ISO 4217) | Text | -
//! `currency_name` | Currency name | Text | -
//! `languages` | Languages spoken (comma separated 2 or 3 letter ISO 639 code with optional hyphen separated country suffix) | Text | -
//! `country_area` | Area of the country (in sq km) | Number | -
//! `country_population` | Population of the country | Number | -
//! `timezone` | Time zone | Text | -
//! `org` | Organization | Text | -
//! `asn` | Autonomous system (AS) | Text | -
//! `country_flag` | Flag of the country | Text (glyph) | -
//!
//! # Example
//!
//! ```toml
//! [[block]]
//! block = "external_ip"
//! format = " $ip $country_code "
//! ```
//!
//! # Notes
//! All the information comes from <https://ipapi.co/json/>
//! Check their documentation here: <https://ipapi.co/api/#complete-location5>
//!
//! The IP is queried, 1) When i3status-rs starts, 2) When a signal is received
//! on D-Bus about a network configuration change, 3) Every 5 minutes. This
//! periodic refresh exists to catch IP updates that don't trigger a notification,
//! for example due to a IP refresh at the router.
//!
//! Flags: They are not icons but unicode glyphs. You will need a font that
//! includes them. Tested with: <https://www.babelstone.co.uk/Fonts/Flags.html>

use zbus::MatchRule;

use super::prelude::*;
use crate::util::{country_flag_from_iso_code, new_system_dbus_connection};

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub format: FormatConfig,
    #[default(300.into())]
    pub interval: Seconds,
    #[default(true)]
    pub with_network_manager: bool,
    #[default(false)]
    pub use_ipv4: bool,
}

pub async fn run(config: &Config, api: &CommonApi) -> Result<()> {
    let format = config.format.with_default(" $ip $country_flag ")?;

    type UpdatesStream = Pin<Box<dyn Stream<Item = ()>>>;
    let mut stream: UpdatesStream = if config.with_network_manager {
        let dbus = new_system_dbus_connection().await?;
        let proxy = zbus::fdo::DBusProxy::new(&dbus)
            .await
            .error("Failed to create DBusProxy")?;
        proxy
            .add_match_rule(
                MatchRule::builder()
                    .msg_type(zbus::message::Type::Signal)
                    .path("/org/freedesktop/NetworkManager")
                    .and_then(|x| x.interface("org.freedesktop.DBus.Properties"))
                    .and_then(|x| x.member("PropertiesChanged"))
                    .unwrap()
                    .build(),
            )
            .await
            .error("Failed to add match")?;
        proxy
            .add_match_rule(
                MatchRule::builder()
                    .msg_type(zbus::message::Type::Signal)
                    .path_namespace("/org/freedesktop/NetworkManager/ActiveConnection")
                    .and_then(|x| x.interface("org.freedesktop.DBus.Properties"))
                    .and_then(|x| x.member("PropertiesChanged"))
                    .unwrap()
                    .build(),
            )
            .await
            .error("Failed to add match")?;
        proxy
            .add_match_rule(
                MatchRule::builder()
                    .msg_type(zbus::message::Type::Signal)
                    .path_namespace("/org/freedesktop/NetworkManager/IP4Config")
                    .and_then(|x| x.interface("org.freedesktop.DBus.Properties"))
                    .and_then(|x| x.member("PropertiesChanged"))
                    .unwrap()
                    .build(),
            )
            .await
            .error("Failed to add match")?;
        let stream: zbus::MessageStream = dbus.into();
        Box::pin(stream.map(|_| ()))
    } else {
        Box::pin(futures::stream::empty())
    };

    let client = if config.use_ipv4 {
        &REQWEST_CLIENT_IPV4
    } else {
        &REQWEST_CLIENT
    };

    loop {
        let fetch_info = || api.find_ip_location(client, Duration::from_secs(0));
        let info = fetch_info.retry(ExponentialBuilder::default()).await?;

        let mut values = map! {
            "ip" => Value::text(info.ip),
            "city" => Value::text(info.city),
            "latitude" => Value::number(info.latitude),
            "longitude" => Value::number(info.longitude),
        };

        macro_rules! map_push_if_some { ($($key:ident: $type:ident),* $(,)?) => {
            $({
                let key = stringify!($key);
                if let Some(value) = info.$key {
                    values.insert(key.into(), Value::$type(value));
                } else if format.contains_key(key) {
                    return Err(Error::new(format!(
                        "The format string contains '{key}', but the {key} field is not provided by {} (an api key may be required)",
                        api.locator_name()
                    )));
                }
            })*
        } }

        map_push_if_some!(
            version: text,
            region: text,
            region_code: text,
            country: text,
            country_name: text,
            country_code_iso3: text,
            country_capital: text,
            country_tld: text,
            continent_code: text,
            postal: text,
            timezone: text,
            utc_offset: text,
            country_calling_code: text,
            currency: text,
            currency_name: text,
            languages: text,
            country_area: number,
            country_population: number,
            asn: text,
            org: text,
        );

        if let Some(country_code) = info.country_code {
            values.insert(
                "country_flag".into(),
                Value::text(country_flag_from_iso_code(&country_code)),
            );
            values.insert("country_code".into(), Value::text(country_code));
        } else if format.contains_key("country_code") || format.contains_key("country_flag") {
            return Err(Error::new(format!(
                "The format string contains 'country_code' or 'country_flag', but the country_code field is not provided by {}",
                api.locator_name()
            )));
        }

        if let Some(in_eu) = info.in_eu {
            if in_eu {
                values.insert("in_eu".into(), Value::flag());
            }
        } else if format.contains_key("in_eu") {
            return Err(Error::new(format!(
                "The format string contains 'in_eu', but the in_eu field is not provided by {}",
                api.locator_name()
            )));
        }

        let mut widget = Widget::new().with_format(format.clone());
        widget.set_values(values);
        api.set_widget(widget)?;

        select! {
            _ = sleep(config.interval.0) => (),
            _ = api.wait_for_update_request() => (),
            _ = stream.next_debounced() => ()
        }
    }
}

//! External IP address and various information about it
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `format` | A string to customise the output of this block. See below for available placeholders. | `" $ip $country_flag "`
//! `interval` | Interval in seconds for automatic updates | `300`
//! `with_network_manager` | If 'true', listen for NetworkManager events and update the IP immediately if there was a change | `true`
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

const API_ENDPOINT: &str = "https://ipapi.co/json/";

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(default)]
pub struct Config {
    format: FormatConfig,
    #[default(300.into())]
    interval: Seconds,
    #[default(true)]
    with_network_manager: bool,
}

pub async fn run(config: Config, mut api: CommonApi) -> Result<()> {
    let mut widget = Widget::new().with_format(config.format.with_default(" $ip $country_flag ")?);

    type UpdatesStream = Pin<Box<dyn Stream<Item = ()>>>;
    let mut stream: UpdatesStream = if config.with_network_manager {
        let dbus = new_system_dbus_connection().await?;
        let proxy = zbus::fdo::DBusProxy::new(&dbus)
            .await
            .error("Failed to create DBusProxy")?;
        proxy
            .add_match_rule(
                MatchRule::builder()
                    .msg_type(zbus::MessageType::Signal)
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
                    .msg_type(zbus::MessageType::Signal)
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
                    .msg_type(zbus::MessageType::Signal)
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

    loop {
        let info = api.recoverable(IPAddressInfo::new).await?;
        let mut values = map! {
            "ip" => Value::text(info.ip),
            "version" => Value::text(info.version),
            "city" => Value::text(info.city),
            "region" => Value::text(info.region),
            "region_code" => Value::text(info.region_code),
            "country" => Value::text(info.country),
            "country_name" => Value::text(info.country_name),
            "country_flag" => Value::text(country_flag_from_iso_code(&info.country_code)),
            "country_code" => Value::text(info.country_code),
            "country_code_iso3" => Value::text(info.country_code_iso3),
            "country_capital" => Value::text(info.country_capital),
            "country_tld" => Value::text(info.country_tld),
            "continent_code" => Value::text(info.continent_code),
            "latitude" => Value::number(info.latitude),
            "longitude" => Value::number(info.longitude),
            "timezone" => Value::text(info.timezone),
            "utc_offset" => Value::text(info.utc_offset),
            "country_calling_code" => Value::text(info.country_calling_code),
            "currency" => Value::text(info.currency),
            "currency_name" => Value::text(info.currency_name),
            "languages" => Value::text(info.languages),
            "country_area" => Value::number(info.country_area),
            "country_population" => Value::number(info.country_population),
            "asn" => Value::text(info.asn),
            "org" => Value::text(info.org),
        };
        info.postal
            .map(|x| values.insert("postal".into(), Value::text(x)));
        if info.in_eu {
            values.insert("in_eu".into(), Value::flag());
        }
        widget.set_values(values);
        api.set_widget(&widget).await?;

        select! {
            _ = sleep(config.interval.0) => (),
            _ = api.wait_for_update_request() => (),
            _ = stream.next() => {
                // avoid too frequent updates
                let _ = tokio::time::timeout(Duration::from_millis(100), async {
                    loop { let _ = stream.next().await; }
                }).await;
            }
        }
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct IPAddressInfo {
    error: bool,
    reason: String,
    ip: String,
    version: String,
    city: String,
    region: String,
    region_code: String,
    country: String,
    country_name: String,
    country_code: String,
    country_code_iso3: String,
    country_capital: String,
    country_tld: String,
    continent_code: String,
    in_eu: bool,
    postal: Option<String>,
    latitude: f64,
    longitude: f64,
    timezone: String,
    utc_offset: String,
    country_calling_code: String,
    currency: String,
    currency_name: String,
    languages: String,
    country_area: f64,
    country_population: f64,
    asn: String,
    org: String,
}

impl IPAddressInfo {
    async fn new() -> Result<Self> {
        let info: Self = REQWEST_CLIENT
            .get(API_ENDPOINT)
            .send()
            .await
            .error("Failed to request current location")?
            .json::<Self>()
            .await
            .error("Failed to parse JSON")?;
        if info.error {
            Err(Error::new(info.reason))
        } else {
            Ok(info)
        }
    }
}

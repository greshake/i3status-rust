//! External IP address and various information about it
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `format` | A string to customise the output of this block. See below for available placeholders. | `" $ip $country_flag "`
//! `interval` | Interval in seconds for automatic updates | `300`
//! `autolocate_interval` | How long in seconds to reuse the last result from the geolocation service instead of contacting it again. Kept small by default so that network changes are picked up promptly. | `1`
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
//! If the service reports rate limiting, the block keeps showing the last
//! known IP and waits for the geolocator's `rate_limit_interval` (10 minutes
//! by default) before asking again.
//!
//! Flags: They are not icons but unicode glyphs. You will need a font that
//! includes them. Tested with: <https://www.babelstone.co.uk/Fonts/Flags.html>

use zbus::MatchRule;

use super::prelude::*;
use crate::geolocator::is_rate_limited;
use crate::util::{country_flag_from_iso_code, new_system_dbus_connection};

make_log_macro!(debug, "external_ip");

/// How long the network-change signal stream must stay quiet before the IP is
/// re-queried; a transition keeps emitting signals for a while, often before
/// connectivity is actually usable.
const SETTLE_QUIET: Duration = Duration::from_secs(1);

/// Upper bound on the settle wait. NetworkManager can keep emitting signals
/// (IP config, DNS, connectivity checks) for many seconds after a transition;
/// without a cap the quiet window keeps sliding and the update is delayed
/// indefinitely. If the network is still not usable when we query, the retry
/// backoff covers it.
const SETTLE_MAX: Duration = Duration::from_secs(3);

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub format: FormatConfig,
    #[default(300.into())]
    pub interval: Seconds,
    /// Unlike the weather block this defaults to 1 second, not `interval`:
    /// picking up a fresh IP right after a network change is the whole point
    /// of this block, so cached results must expire quickly.
    #[default(1.into())]
    pub autolocate_interval: Seconds,
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
        // If the D-Bus connection dies the stream ends; without the chained
        // pending stream, polling it again would resolve instantly forever,
        // turning the loop below into a busy loop of API requests.
        Box::pin(stream.map(|_| ()).chain(futures::stream::pending()))
    } else {
        Box::pin(futures::stream::pending())
    };

    let client = if config.use_ipv4 {
        &REQWEST_CLIENT_IPV4
    } else {
        &REQWEST_CLIENT
    };

    loop {
        let fetch_start = tokio::time::Instant::now();
        let info = match api
            .find_ip_location(client, config.autolocate_interval.0)
            .await
        {
            Ok(info) => info,
            Err(err) if is_rate_limited(&err) => {
                // Keep displaying the last known IP and try again once the
                // geolocator's rate limit interval has passed (plus a margin
                // so we don't wake up just before it expires); erroring out
                // here would make the block restart machinery re-query every
                // `error_interval` seconds, which keeps the rate limit from
                // ever lifting.
                sleep(api.locator_rate_limit_interval() + Duration::from_secs(1)).await;
                continue;
            }
            Err(err) => return Err(err),
        };
        debug!("got {} after {:?}", info.ip, fetch_start.elapsed());

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
            _ = stream.next_debounced() => {
                // Wait for the burst of signals to die down before re-querying,
                // so that one network transition results in one request, made
                // once the new connection is likely up.
                let settle_start = tokio::time::Instant::now();
                while let Ok(Some(_)) = tokio::time::timeout(SETTLE_QUIET, stream.next()).await {
                    if settle_start.elapsed() >= SETTLE_MAX {
                        break;
                    }
                }
                debug!("signals settled after {:?}", settle_start.elapsed());
            }
        }
    }
}

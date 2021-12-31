use std::thread;
use std::time::Instant;

use crossbeam_channel::Sender;
use dbus::ffidisp::{BusType, Connection, ConnectionItem};
use serde::{Deserialize as des, Serialize as ser};
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::SharedConfig;
use crate::errors::*;
use crate::formatting::value::Value;
use crate::formatting::FormatTemplate;
use crate::http;
use crate::scheduler::Task;
use crate::util::country_flag_from_iso_code;
use crate::widgets::text::TextWidget;
use crate::widgets::{I3BarWidget, State};
use crate::Duration;

const API_ENDPOINT: &str = "https://ipapi.co/json/";
const BLOCK_NAME: &str = "external_ip";

#[derive(ser, des, Default)]
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

pub struct ExternalIP {
    id: usize,
    output: TextWidget,
    format: FormatTemplate,
    refresh_interval_success: u64,
    refresh_interval_failure: u64,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct ExternalIPConfig {
    /// External IP formatter.
    pub format: FormatTemplate,
    pub interval: u64,
    pub error_interval: u64,
    pub with_network_manager: bool,
}

impl Default for ExternalIPConfig {
    fn default() -> Self {
        Self {
            format: FormatTemplate::default(),
            interval: 300,
            error_interval: 15,
            with_network_manager: true,
        }
    }
}

impl ConfigBlock for ExternalIP {
    type Config = ExternalIPConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        shared_config: SharedConfig,
        send: Sender<Task>,
    ) -> Result<Self> {
        if block_config.with_network_manager {
            thread::Builder::new()
                .name("externalip".into())
                .spawn(move || {
                    let c = Connection::get_private(BusType::System).unwrap();
                    c.add_match(
                        "type='signal',\
                    path='/org/freedesktop/NetworkManager',\
                    interface='org.freedesktop.DBus.Properties',\
                    member='PropertiesChanged'",
                    )
                    .unwrap();
                    c.add_match(
                        "type='signal',\
                    path_namespace='/org/freedesktop/NetworkManager/ActiveConnection',\
                    interface='org.freedesktop.DBus.Properties',\
                    member='PropertiesChanged'",
                    )
                    .unwrap();
                    c.add_match(
                        "type='signal',\
                    path_namespace='/org/freedesktop/NetworkManager/IP4Config',\
                    interface='org.freedesktop.DBus',\
                    member='PropertiesChanged'",
                    )
                    .unwrap();

                    loop {
                        let timeout = 300_000;

                        for event in c.iter(timeout) {
                            match event {
                                ConnectionItem::Nothing => (),
                                _ => {
                                    send.send(Task {
                                        id,
                                        update_time: Instant::now(),
                                    })
                                    .unwrap();
                                }
                            }
                        }
                    }
                })
                .unwrap();
        }
        Ok(ExternalIP {
            id,
            output: TextWidget::new(id, 0, shared_config),
            format: block_config.format.with_default("{ip} {country_flag}")?,
            refresh_interval_success: block_config.interval,
            refresh_interval_failure: block_config.error_interval,
        })
    }
}

impl Block for ExternalIP {
    fn id(&self) -> usize {
        self.id
    }

    fn update(&mut self) -> Result<Option<Update>> {
        let (external_ip, success) = {
            let ip_info: Result<IPAddressInfo> =
                match http::http_get_json(API_ENDPOINT, Some(Duration::from_secs(3)), vec![]) {
                    Ok(ip_info_json) => serde_json::from_value(ip_info_json.content)
                        .block_error(BLOCK_NAME, "Failed to decode JSON"),
                    _ => Err(BlockError(
                        BLOCK_NAME.to_string(),
                        "Failed to contact API".to_string(),
                    )),
                };
            match ip_info {
                Ok(ip_info) => match ip_info.error {
                    false => {
                        self.output.set_state(State::Idle);
                        let flag = country_flag_from_iso_code(ip_info.country_code.as_str());
                        let values = map!(
                            "ip" => Value::from_string (ip_info.ip),
                            "version" => Value::from_string (ip_info.version),
                            "city" => Value::from_string (ip_info.city),
                            "region" => Value::from_string (ip_info.region),
                            "region_code" => Value::from_string (ip_info.region_code),
                            "country" => Value::from_string (ip_info.country),
                            "country_name" => Value::from_string (ip_info.country_name),
                            "country_code" => Value::from_string (ip_info.country_code),
                            "country_code_iso3" => Value::from_string (ip_info.country_code_iso3),
                            "country_capital" => Value::from_string (ip_info.country_capital),
                            "country_tld" => Value::from_string (ip_info.country_tld),
                            "continent_code" => Value::from_string (ip_info.continent_code),
                            "in_eu" => Value::from_boolean (ip_info.in_eu),
                            "postal" => Value::from_string (ip_info.postal.unwrap_or_else(|| "No postal code".to_string())),
                            "latitude" => Value::from_float (ip_info.latitude),
                            "longitude" => Value::from_float (ip_info.longitude),
                            "timezone" => Value::from_string (ip_info.timezone),
                            "utc_offset" => Value::from_string (ip_info.utc_offset),
                            "country_calling_code" => Value::from_string (ip_info.country_calling_code),
                            "currency" => Value::from_string (ip_info.currency),
                            "currency_name" => Value::from_string (ip_info.currency_name),
                            "languages" => Value::from_string (ip_info.languages),
                            "country_area" => Value::from_float (ip_info.country_area),
                            "country_population" => Value::from_float (ip_info.country_population),
                            "asn" => Value::from_string (ip_info.asn),
                            "org" => Value::from_string (ip_info.org),
                            "country_flag" => Value::from_string(flag),
                        );
                        let s = self.format.render(&values)?;
                        (s.0, true)
                    }
                    true => {
                        self.output.set_state(State::Critical);
                        (format!("Error: {}", ip_info.reason), false)
                    }
                },
                Err(err) => {
                    self.output.set_state(State::Critical);
                    (err.to_string(), false)
                }
            }
        };

        self.output.set_text(external_ip);
        match success {
            /* The external IP address can change without triggering a
             * notification (for example a refresh between the router and
             * the ISP) so check from time to time even on success */
            true => Ok(Some(
                Duration::from_secs(self.refresh_interval_success).into(),
            )),
            false => Ok(Some(
                Duration::from_secs(self.refresh_interval_failure).into(),
            )),
        }
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.output]
    }
}
